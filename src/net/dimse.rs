//! Pure-Rust DICOM Upper Layer (PS 3.8) & DIMSE-C SCU for testing connectivity
//! (`C-ECHO`), querying studies (`C-FIND`), and sending studies (`C-STORE`) to
//! standard DICOM PACS peers (`network-pacs` area in `PORTING_MAP.md`).

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};

use super::{PacsNode, QueryFilter, StudyQueryResult};

const VERIFICATION_SOP_CLASS: &[u8] = b"1.2.840.10008.1.1";
const STUDY_ROOT_FIND_SOP_CLASS: &[u8] = b"1.2.840.10008.5.1.4.1.2.2.1";
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

/// Builds a DICOM `A-ASSOCIATE-RQ` PDU (PS 3.8 §9.3.2) proposing one or more
/// `(presentation_context_id, abstract_syntax_uid, transfer_syntax_uid)` triples.
pub fn build_associate_rq_contexts(
    called_ae: &str,
    calling_ae: &str,
    contexts: &[(u8, &[u8], &[u8])],
) -> Vec<u8> {
    let mut body = Vec::with_capacity(256);
    body.extend_from_slice(&1_u16.to_be_bytes()); // Protocol Version = 1
    body.extend_from_slice(&[0u8; 2]); // Reserved
    body.extend_from_slice(&pad_ae(called_ae));
    body.extend_from_slice(&pad_ae(calling_ae));
    body.extend_from_slice(&[0u8; 32]); // Reserved

    // Application Context Item (0x10)
    push_item(&mut body, 0x10, APP_CONTEXT_UID);

    for &(pc_id, abstract_syntax, transfer_syntax) in contexts {
        let mut pc = Vec::new();
        pc.push(pc_id);
        pc.extend_from_slice(&[0, 0, 0]);
        push_item(&mut pc, 0x30, abstract_syntax);
        push_item(&mut pc, 0x40, transfer_syntax);
        push_item(&mut body, 0x20, &pc);
    }

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

/// Builds a DICOM `A-ASSOCIATE-RQ` PDU proposing Verification SOP Class.
pub fn build_associate_rq(called_ae: &str, calling_ae: &str) -> Vec<u8> {
    build_associate_rq_contexts(
        called_ae,
        calling_ae,
        &[(1, VERIFICATION_SOP_CLASS, IMPLICIT_VR_LE)],
    )
}

fn push_implicit_elem(buf: &mut Vec<u8>, group: u16, elem: u16, val: &[u8], pad_byte: u8) {
    buf.extend_from_slice(&group.to_le_bytes());
    buf.extend_from_slice(&elem.to_le_bytes());
    let pad = val.len() % 2;
    let len = (val.len() + pad) as u32;
    buf.extend_from_slice(&len.to_le_bytes());
    buf.extend_from_slice(val);
    if pad == 1 {
        buf.push(pad_byte);
    }
}

fn wrap_pdata(pc_id: u8, msg_ctrl: u8, payload: &[u8]) -> Vec<u8> {
    let pdv_len = (2 + payload.len()) as u32;
    let mut body = Vec::with_capacity(4 + pdv_len as usize);
    body.extend_from_slice(&pdv_len.to_be_bytes());
    body.push(pc_id);
    body.push(msg_ctrl);
    body.extend_from_slice(payload);

    let mut pdu = Vec::with_capacity(6 + body.len());
    pdu.push(0x04); // P-DATA-TF
    pdu.push(0x00);
    pdu.extend_from_slice(&(body.len() as u32).to_be_bytes());
    pdu.extend_from_slice(&body);
    pdu
}

fn finish_command_group(cmd_body: &[u8]) -> Vec<u8> {
    let mut full = Vec::with_capacity(12 + cmd_body.len());
    push_implicit_elem(
        &mut full,
        0x0000,
        0x0000,
        &(cmd_body.len() as u32).to_le_bytes(),
        0x00,
    );
    full.extend_from_slice(cmd_body);
    full
}

/// Builds a `P-DATA-TF` PDU carrying a DIMSE `C-ECHO-RQ` command dataset.
pub fn build_c_echo_rq_pdu() -> Vec<u8> {
    let mut cmd = Vec::new();
    push_implicit_elem(&mut cmd, 0x0000, 0x0002, VERIFICATION_SOP_CLASS, 0x00);
    push_implicit_elem(&mut cmd, 0x0000, 0x0100, &0x0030_u16.to_le_bytes(), 0x00); // C-ECHO-RQ
    push_implicit_elem(&mut cmd, 0x0000, 0x0110, &1_u16.to_le_bytes(), 0x00); // MessageID = 1
    push_implicit_elem(&mut cmd, 0x0000, 0x0800, &0x0101_u16.to_le_bytes(), 0x00); // No Data Set
    wrap_pdata(1, 0x03, &finish_command_group(&cmd))
}

/// Builds `(command_pdu, identifier_dataset_pdu)` for a Study-Root `C-FIND-RQ`.
pub fn build_c_find_rq_pdus(filter: &QueryFilter) -> (Vec<u8>, Vec<u8>) {
    let mut cmd = Vec::new();
    push_implicit_elem(&mut cmd, 0x0000, 0x0002, STUDY_ROOT_FIND_SOP_CLASS, 0x00);
    push_implicit_elem(&mut cmd, 0x0000, 0x0100, &0x0020_u16.to_le_bytes(), 0x00); // C-FIND-RQ
    push_implicit_elem(&mut cmd, 0x0000, 0x0110, &1_u16.to_le_bytes(), 0x00);
    push_implicit_elem(&mut cmd, 0x0000, 0x0700, &0x0000_u16.to_le_bytes(), 0x00); // Priority = MEDIUM
    push_implicit_elem(&mut cmd, 0x0000, 0x0800, &0x0000_u16.to_le_bytes(), 0x00); // Has Data Set

    let mut ds = Vec::new();
    push_implicit_elem(&mut ds, 0x0008, 0x0020, filter.study_date.trim().as_bytes(), b' ');
    push_implicit_elem(&mut ds, 0x0008, 0x0050, filter.accession_number.trim().as_bytes(), b' ');
    push_implicit_elem(&mut ds, 0x0008, 0x0052, b"STUDY", b' ');
    push_implicit_elem(&mut ds, 0x0008, 0x0061, filter.modality.trim().as_bytes(), b' ');
    push_implicit_elem(&mut ds, 0x0008, 0x1030, b"", b' ');
    push_implicit_elem(&mut ds, 0x0010, 0x0010, filter.patient_name.trim().as_bytes(), b' ');
    push_implicit_elem(&mut ds, 0x0010, 0x0020, filter.patient_id.trim().as_bytes(), b' ');
    push_implicit_elem(&mut ds, 0x0020, 0x000D, b"", 0x00);
    push_implicit_elem(&mut ds, 0x0020, 0x1206, b"", b' ');
    push_implicit_elem(&mut ds, 0x0020, 0x1208, b"", b' ');

    (
        wrap_pdata(1, 0x03, &finish_command_group(&cmd)),
        wrap_pdata(1, 0x02, &ds),
    )
}

/// Parses an Implicit VR Little Endian dataset into a `StudyQueryResult`.
pub fn parse_implicit_dataset_study(data: &[u8]) -> Option<StudyQueryResult> {
    let mut res = StudyQueryResult::default();
    let mut pos = 0;
    while pos + 8 <= data.len() {
        let group = u16::from_le_bytes([data[pos], data[pos + 1]]);
        let elem = u16::from_le_bytes([data[pos + 2], data[pos + 3]]);
        let len = u32::from_le_bytes([data[pos + 4], data[pos + 5], data[pos + 6], data[pos + 7]]) as usize;
        pos += 8;
        if len == 0xFFFF_FFFF || pos + len > data.len() {
            break;
        }
        let val = &data[pos..pos + len];
        pos += len;
        let text = String::from_utf8_lossy(val)
            .trim_matches(|c: char| c == '\0' || c.is_ascii_whitespace())
            .to_string();
        match (group, elem) {
            (0x0008, 0x0020) => res.study_date = text,
            (0x0008, 0x0050) => res.accession_number = text,
            (0x0008, 0x0061) => res.modalities = text,
            (0x0008, 0x1030) => res.description = text,
            (0x0010, 0x0010) => res.patient_name = text,
            (0x0010, 0x0020) => res.patient_id = text,
            (0x0020, 0x000D) => res.study_uid = text,
            (0x0020, 0x1206) => res.num_series = text.parse().unwrap_or(0),
            (0x0020, 0x1208) => res.num_instances = text.parse().unwrap_or(0),
            _ => {}
        }
    }
    if res.study_uid.is_empty() {
        None
    } else {
        Some(res)
    }
}

fn parse_dimse_status(cmd: &[u8]) -> Option<u16> {
    let mut pos = 0;
    while pos + 8 <= cmd.len() {
        let group = u16::from_le_bytes([cmd[pos], cmd[pos + 1]]);
        let elem = u16::from_le_bytes([cmd[pos + 2], cmd[pos + 3]]);
        let len = u32::from_le_bytes([cmd[pos + 4], cmd[pos + 5], cmd[pos + 6], cmd[pos + 7]]) as usize;
        pos += 8;
        if pos + len > cmd.len() {
            break;
        }
        if group == 0x0000 && elem == 0x0900 && len >= 2 {
            return Some(u16::from_le_bytes([cmd[pos], cmd[pos + 1]]));
        }
        pos += len;
    }
    None
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

fn connect_node(node: &PacsNode) -> Result<TcpStream> {
    if !node.is_valid() {
        return Err(anyhow!("Invalid PACS node configuration (check AE Title, host, and port)"));
    }
    let addr = format!("{}:{}", node.host.trim(), node.port)
        .to_socket_addrs()
        .with_context(|| format!("could not resolve {}:{}", node.host, node.port))?
        .next()
        .ok_or_else(|| anyhow!("no socket address resolved for {}", node.host))?;
    let stream = TcpStream::connect_timeout(&addr, Duration::from_secs(6))
        .with_context(|| format!("connection to {addr} failed"))?;
    stream.set_read_timeout(Some(Duration::from_secs(15)))?;
    stream.set_write_timeout(Some(Duration::from_secs(15)))?;
    Ok(stream)
}

fn send_release(stream: &mut TcpStream) {
    let _ = stream.write_all(&[0x05, 0x00, 0x00, 0x00, 0x00, 0x04, 0, 0, 0, 0]);
}

/// Reads one complete DIMSE message (command or dataset) across one or more `P-DATA-TF` PDUs.
fn read_dimse_fragment(stream: &mut TcpStream) -> Result<(bool, Vec<u8>)> {
    let mut acc = Vec::new();
    let mut is_cmd = false;
    loop {
        let (pdu_type, body) = read_pdu(stream)?;
        if pdu_type != 0x04 {
            return Err(anyhow!("Unexpected DICOM PDU 0x{pdu_type:02X} while reading P-DATA-TF"));
        }
        let mut pos = 0;
        let mut done = false;
        while pos + 6 <= body.len() {
            let pdv_len = u32::from_be_bytes([body[pos], body[pos + 1], body[pos + 2], body[pos + 3]]) as usize;
            pos += 4;
            if pdv_len < 2 || pos + pdv_len > body.len() {
                break;
            }
            let msg_ctrl = body[pos + 1];
            is_cmd = (msg_ctrl & 0x01) != 0;
            let last = (msg_ctrl & 0x02) != 0;
            acc.extend_from_slice(&body[pos + 2..pos + pdv_len]);
            pos += pdv_len;
            if last {
                done = true;
            }
        }
        if done {
            return Ok((is_cmd, acc));
        }
    }
}

/// Runs a DICOM `C-ECHO` against `node`, returning round-trip latency in milliseconds.
pub fn c_echo(node: &PacsNode) -> Result<u128> {
    let start = Instant::now();
    let mut stream = connect_node(node)?;

    let assoc_rq = build_associate_rq(&node.ae_title, &node.local_ae_title);
    stream.write_all(&assoc_rq)?;
    let (pdu_type, _) = read_pdu(&mut stream)?;
    if pdu_type == 0x03 {
        return Err(anyhow!("PACS rejected association (A-ASSOCIATE-RJ) — check AE Titles"));
    }
    if pdu_type != 0x02 {
        return Err(anyhow!("Unexpected DICOM PDU type 0x{pdu_type:02X} (expected A-ASSOCIATE-AC)"));
    }

    stream.write_all(&build_c_echo_rq_pdu())?;
    let (resp_type, _) = read_pdu(&mut stream)?;
    if resp_type != 0x04 {
        return Err(anyhow!("Unexpected response PDU 0x{resp_type:02X} to C-ECHO-RQ"));
    }

    send_release(&mut stream);
    Ok(start.elapsed().as_millis())
}

/// Runs a Study-Root `C-FIND` query against `node`.
pub fn c_find_studies(node: &PacsNode, filter: &QueryFilter) -> Result<Vec<StudyQueryResult>> {
    let mut stream = connect_node(node)?;
    let assoc_rq = build_associate_rq_contexts(
        &node.ae_title,
        &node.local_ae_title,
        &[(1, STUDY_ROOT_FIND_SOP_CLASS, IMPLICIT_VR_LE)],
    );
    stream.write_all(&assoc_rq)?;
    let (pdu_type, _) = read_pdu(&mut stream)?;
    if pdu_type == 0x03 {
        return Err(anyhow!("PACS rejected association for C-FIND — check AE Titles"));
    }
    if pdu_type != 0x02 {
        return Err(anyhow!("Unexpected PDU 0x{pdu_type:02X} (expected A-ASSOCIATE-AC)"));
    }

    let (cmd_pdu, ds_pdu) = build_c_find_rq_pdus(filter);
    stream.write_all(&cmd_pdu)?;
    stream.write_all(&ds_pdu)?;

    let mut results = Vec::new();
    loop {
        let (is_cmd, cmd_bytes) = read_dimse_fragment(&mut stream)?;
        if !is_cmd {
            continue;
        }
        let status = parse_dimse_status(&cmd_bytes).unwrap_or(0x0000);
        if status == 0xFF00 || status == 0xFF01 {
            let (_is_cmd2, ds_bytes) = read_dimse_fragment(&mut stream)?;
            if let Some(st) = parse_implicit_dataset_study(&ds_bytes) {
                results.push(st);
            }
        } else {
            break;
        }
    }

    send_release(&mut stream);
    Ok(results)
}

/// Sends DICOM files to `node` using DIMSE `C-STORE`.
pub fn c_store_files(node: &PacsNode, paths: &[PathBuf]) -> Result<usize> {
    if paths.is_empty() {
        return Ok(0);
    }
    struct Prepared {
        key: (String, String),
        sop_class: String,
        sop_instance: String,
        dataset: Vec<u8>,
    }
    let mut prepared = Vec::with_capacity(paths.len());
    let mut ctx_to_pc: HashMap<(String, String), u8> = HashMap::new();
    let mut next_pc: u8 = 1;
    for p in paths {
        let obj = dicom_object::open_file(p)
            .with_context(|| format!("failed to open DICOM file {}", p.display()))?;
        let ts_uid = obj
            .meta()
            .transfer_syntax
            .trim_matches('\0')
            .trim()
            .to_string();
        let ts_uid = if ts_uid.is_empty() {
            "1.2.840.10008.1.2".to_string()
        } else {
            ts_uid
        };
        let sop_class = obj
            .element_by_name("SOPClassUID")
            .ok()
            .and_then(|e| e.to_str().ok())
            .map(|s| s.trim_matches('\0').trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "1.2.840.10008.5.1.4.1.1.2".to_string());
        let sop_instance = obj
            .element_by_name("SOPInstanceUID")
            .ok()
            .and_then(|e| e.to_str().ok())
            .map(|s| s.trim_matches('\0').trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| format!("1.2.826.0.1.3680043.10.1234.{}", prepared.len() + 1));
        let key = (sop_class.clone(), ts_uid);
        if !ctx_to_pc.contains_key(&key) && next_pc < 250 {
            ctx_to_pc.insert(key.clone(), next_pc);
            next_pc += 2; // odd presentation context IDs
        }
        let mut dataset = Vec::new();
        obj.write_dataset(&mut dataset)
            .with_context(|| format!("failed to encode dataset for {}", p.display()))?;
        prepared.push(Prepared {
            key,
            sop_class,
            sop_instance,
            dataset,
        });
    }

    let contexts_owned: Vec<(u8, Vec<u8>, Vec<u8>)> = ctx_to_pc
        .iter()
        .map(|((sop, ts), &pc)| (pc, sop.as_bytes().to_vec(), ts.as_bytes().to_vec()))
        .collect();
    let contexts_ref: Vec<(u8, &[u8], &[u8])> = contexts_owned
        .iter()
        .map(|(pc, sop, ts)| (*pc, sop.as_slice(), ts.as_slice()))
        .collect();

    let mut stream = connect_node(node)?;
    let assoc_rq =
        build_associate_rq_contexts(&node.ae_title, &node.local_ae_title, &contexts_ref);
    stream.write_all(&assoc_rq)?;
    let (pdu_type, _) = read_pdu(&mut stream)?;
    if pdu_type == 0x03 {
        return Err(anyhow!("PACS rejected association for C-STORE — check AE Titles"));
    }
    if pdu_type != 0x02 {
        return Err(anyhow!("Unexpected PDU 0x{pdu_type:02X} (expected A-ASSOCIATE-AC)"));
    }

    let mut sent = 0usize;
    for (idx, item) in prepared.iter().enumerate() {
        let Some(&pc_id) = ctx_to_pc.get(&item.key) else {
            continue;
        };
        let msg_id = (idx + 1) as u16;
        let mut cmd = Vec::new();
        push_implicit_elem(&mut cmd, 0x0000, 0x0002, item.sop_class.as_bytes(), 0x00);
        push_implicit_elem(&mut cmd, 0x0000, 0x0100, &0x0001_u16.to_le_bytes(), 0x00); // C-STORE-RQ
        push_implicit_elem(&mut cmd, 0x0000, 0x0110, &msg_id.to_le_bytes(), 0x00);
        push_implicit_elem(&mut cmd, 0x0000, 0x0700, &0x0000_u16.to_le_bytes(), 0x00);
        push_implicit_elem(&mut cmd, 0x0000, 0x0800, &0x0000_u16.to_le_bytes(), 0x00);
        push_implicit_elem(&mut cmd, 0x0000, 0x1000, item.sop_instance.as_bytes(), 0x00);
        stream.write_all(&wrap_pdata(pc_id, 0x03, &finish_command_group(&cmd)))?;

        // Stream dataset in 14 KiB chunks (under 16 KiB max PDU)
        const CHUNK: usize = 14 * 1024;
        let total = item.dataset.len();
        let mut off = 0;
        while off < total {
            let end = (off + CHUNK).min(total);
            let is_last = end == total;
            let ctrl = if is_last { 0x02 } else { 0x00 };
            stream.write_all(&wrap_pdata(pc_id, ctrl, &item.dataset[off..end]))?;
            off = end;
        }

        let (_is_cmd, rsp) = read_dimse_fragment(&mut stream)?;
        let status = parse_dimse_status(&rsp).unwrap_or(0xFFFF);
        if status == 0x0000 || status == 0xB000 || status == 0xB007 || status == 0xB006 {
            sent += 1;
        }
    }

    send_release(&mut stream);
    Ok(sent)
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

    #[test]
    fn c_find_rq_and_dataset_parser_roundtrip() {
        let f = QueryFilter {
            patient_name: "DOE^JOHN".into(),
            modality: "CT".into(),
            study_date: "20261121".into(),
            ..Default::default()
        };
        let (cmd_pdu, ds_pdu) = build_c_find_rq_pdus(&f);
        assert_eq!(cmd_pdu[0], 0x04);
        assert_eq!(ds_pdu[0], 0x04);
        assert_eq!(ds_pdu[11], 0x02); // Dataset + Last Fragment

        let mut fake_rsp = Vec::new();
        push_implicit_elem(&mut fake_rsp, 0x0020, 0x000D, b"1.2.3.4.5", 0x00);
        push_implicit_elem(&mut fake_rsp, 0x0010, 0x0010, b"DOE^JOHN", b' ');
        push_implicit_elem(&mut fake_rsp, 0x0008, 0x0061, b"CT", b' ');
        push_implicit_elem(&mut fake_rsp, 0x0020, 0x1206, b"3", b' ');
        let parsed = parse_implicit_dataset_study(&fake_rsp).unwrap();
        assert_eq!(parsed.study_uid, "1.2.3.4.5");
        assert_eq!(parsed.patient_name, "DOE^JOHN");
        assert_eq!(parsed.modalities, "CT");
        assert_eq!(parsed.num_series, 3);
    }
}
