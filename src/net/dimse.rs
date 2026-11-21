//! Pure-Rust DICOM Upper Layer (PS 3.8) C-ECHO SCU for testing connectivity
//! to standard DICOM PACS peers (`network-pacs` area in `PORTING_MAP.md`).

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};

use super::PacsNode;

const VERIFICATION_SOP_CLASS: &[u8] = b"1.2.840.10008.1.1";
const IMPLICIT_VR_LE: &[u8] = b"1.2.840.10008.1.2";
const APP_CONTEXT_UID: &[u8] = b"1.2.840.10008.3.1.1.1";
const IMPL_CLASS_UID: &[u8] = b"1.2.826.0.1.3680043.10.1234";

fn pad_ae(ae: &str) -> [u8; 16] {
    let mut out = [b' '; 16];
    for (dst, &b) in out.iter_mut().zip(ae.trim().as_bytes().iter().take(16)) {
        *dst = b;
    }
    out
}

fn push_item(buf: &mut Vec<u8>, item_type: u8, payload: &[u8]) {
    buf.push(item_type);
    buf.push(0x00);
    buf.extend_from_slice(&(payload.len() as u16).to_be_bytes());
    buf.extend_from_slice(payload);
}

/// Builds a DICOM `A-ASSOCIATE-RQ` PDU (PS 3.8 §9.3.2) proposing Verification SOP Class.
pub fn build_associate_rq(called_ae: &str, calling_ae: &str) -> Vec<u8> {
    let mut body = Vec::with_capacity(256);
    body.extend_from_slice(&1_u16.to_be_bytes()); // Protocol Version = 1
    body.extend_from_slice(&[0u8; 2]); // Reserved
    body.extend_from_slice(&pad_ae(called_ae));
    body.extend_from_slice(&pad_ae(calling_ae));
    body.extend_from_slice(&[0u8; 32]); // Reserved

    // Application Context Item (0x10)
    push_item(&mut body, 0x10, APP_CONTEXT_UID);

    // Presentation Context Item (0x20): ID=1, Verification + Implicit VR LE
    let mut pc = Vec::new();
    pc.push(1); // Presentation Context ID
    pc.extend_from_slice(&[0, 0, 0]);
    push_item(&mut pc, 0x30, VERIFICATION_SOP_CLASS);
    push_item(&mut pc, 0x40, IMPLICIT_VR_LE);
    push_item(&mut body, 0x20, &pc);

    // User Information Item (0x50): Max PDU Length (16384) + Implementation Class UID
    let mut ui = Vec::new();
    push_item(&mut ui, 0x51, &16384_u32.to_be_bytes());
    push_item(&mut ui, 0x52, IMPL_CLASS_UID);
    push_item(&mut body, 0x50, &ui);

    let mut pdu = Vec::with_capacity(6 + body.len());
    pdu.push(0x01); // A-ASSOCIATE-RQ
    pdu.push(0x00);
    pdu.extend_from_slice(&(body.len() as u32).to_be_bytes());
    pdu.extend_from_slice(&body);
    pdu
}

fn push_implicit_elem(buf: &mut Vec<u8>, group: u16, elem: u16, val: &[u8]) {
    buf.extend_from_slice(&group.to_le_bytes());
    buf.extend_from_slice(&elem.to_le_bytes());
    let pad = val.len() % 2;
    let len = (val.len() + pad) as u32;
    buf.extend_from_slice(&len.to_le_bytes());
    buf.extend_from_slice(val);
    if pad == 1 {
        buf.push(0x00);
    }
}

/// Builds a `P-DATA-TF` PDU carrying a DIMSE `C-ECHO-RQ` command dataset.
pub fn build_c_echo_rq_pdu() -> Vec<u8> {
    let mut cmd = Vec::new();
    push_implicit_elem(&mut cmd, 0x0000, 0x0002, VERIFICATION_SOP_CLASS);
    push_implicit_elem(&mut cmd, 0x0000, 0x0100, &0x0030_u16.to_le_bytes()); // C-ECHO-RQ
    push_implicit_elem(&mut cmd, 0x0000, 0x0110, &1_u16.to_le_bytes()); // MessageID = 1
    push_implicit_elem(&mut cmd, 0x0000, 0x0800, &0x0101_u16.to_le_bytes()); // No Data Set

    // Prepend CommandGroupLength (0000,0000)
    let mut full_cmd = Vec::with_capacity(12 + cmd.len());
    push_implicit_elem(&mut full_cmd, 0x0000, 0x0000, &(cmd.len() as u32).to_le_bytes());
    full_cmd.extend_from_slice(&cmd);

    // PDV Item: length (4) + ctx_id (1) + msg_ctrl (0x03 = command + last fragment) + data
    let pdv_len = (2 + full_cmd.len()) as u32;
    let mut body = Vec::with_capacity(4 + pdv_len as usize);
    body.extend_from_slice(&pdv_len.to_be_bytes());
    body.push(1); // Presentation Context ID 1
    body.push(0x03); // Command + Last Fragment
    body.extend_from_slice(&full_cmd);

    let mut pdu = Vec::with_capacity(6 + body.len());
    pdu.push(0x04); // P-DATA-TF
    pdu.push(0x00);
    pdu.extend_from_slice(&(body.len() as u32).to_be_bytes());
    pdu.extend_from_slice(&body);
    pdu
}

fn read_pdu(stream: &mut TcpStream) -> Result<(u8, Vec<u8>)> {
    let mut hdr = [0u8; 6];
    stream.read_exact(&mut hdr).context("failed to read DICOM PDU header")?;
    let pdu_type = hdr[0];
    let len = u32::from_be_bytes([hdr[2], hdr[3], hdr[4], hdr[5]]) as usize;
    if len > 16 * 1024 * 1024 {
        return Err(anyhow!("unexpected DICOM PDU length {len}"));
    }
    let mut payload = vec![0u8; len];
    stream.read_exact(&mut payload).context("failed to read DICOM PDU body")?;
    Ok((pdu_type, payload))
}

/// Runs a DICOM `C-ECHO` against `node`, returning round-trip latency in milliseconds.
pub fn c_echo(node: &PacsNode) -> Result<u128> {
    if !node.is_valid() {
        return Err(anyhow!("Invalid PACS node configuration (check AE Title, host, and port)"));
    }
    let addr = format!("{}:{}", node.host.trim(), node.port)
        .to_socket_addrs()
        .with_context(|| format!("could not resolve {}:{}", node.host, node.port))?
        .next()
        .ok_or_else(|| anyhow!("no socket address resolved for {}", node.host))?;

    let start = Instant::now();
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(5))
        .with_context(|| format!("connection to {addr} failed"))?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;

    // 1. A-ASSOCIATE-RQ -> A-ASSOCIATE-AC
    let assoc_rq = build_associate_rq(&node.ae_title, &node.local_ae_title);
    stream.write_all(&assoc_rq)?;
    let (pdu_type, _) = read_pdu(&mut stream)?;
    if pdu_type == 0x03 {
        return Err(anyhow!("PACS rejected association (A-ASSOCIATE-RJ) — check AE Titles"));
    }
    if pdu_type != 0x02 {
        return Err(anyhow!("Unexpected DICOM PDU type 0x{pdu_type:02X} (expected A-ASSOCIATE-AC)"));
    }

    // 2. C-ECHO-RQ -> C-ECHO-RSP
    let echo_rq = build_c_echo_rq_pdu();
    stream.write_all(&echo_rq)?;
    let (resp_type, _) = read_pdu(&mut stream)?;
    if resp_type != 0x04 {
        return Err(anyhow!("Unexpected response PDU 0x{resp_type:02X} to C-ECHO-RQ"));
    }

    // 3. A-RELEASE-RQ (best-effort)
    let _ = stream.write_all(&[0x05, 0x00, 0x00, 0x00, 0x00, 0x04, 0, 0, 0, 0]);
    Ok(start.elapsed().as_millis())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn associate_rq_encodes_header_ae_titles_and_verification_uid() {
        let pdu = build_associate_rq("ORTHANC", "SEKHMETX");
        assert_eq!(pdu[0], 0x01); // A-ASSOCIATE-RQ
        let declared_len = u32::from_be_bytes([pdu[2], pdu[3], pdu[4], pdu[5]]) as usize;
        assert_eq!(declared_len + 6, pdu.len());
        assert_eq!(&pdu[10..26], b"ORTHANC         ");
        assert_eq!(&pdu[26..42], b"SEKHMETX        ");
        assert!(pdu.windows(VERIFICATION_SOP_CLASS.len()).any(|w| w == VERIFICATION_SOP_CLASS));
    }

    #[test]
    fn c_echo_rq_pdu_is_valid_pdata_command() {
        let pdu = build_c_echo_rq_pdu();
        assert_eq!(pdu[0], 0x04); // P-DATA-TF
        let declared_len = u32::from_be_bytes([pdu[2], pdu[3], pdu[4], pdu[5]]) as usize;
        assert_eq!(declared_len + 6, pdu.len());
        assert_eq!(pdu[10], 1); // Presentation Context ID 1
        assert_eq!(pdu[11], 0x03); // Command + Last Fragment
    }
}
