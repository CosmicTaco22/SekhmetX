const VM_DOC_HELPER: &str = include_str!("scripts/vm_doc.js");

fn inject_payload(template: &str, payload_json: &str) -> String {
    template
        .replacen("__VM_DOC__", VM_DOC_HELPER, 1)
        .replacen("__PAYLOAD__", payload_json, 1)
}

pub(super) fn build_picker_js(payload_json: &str) -> String {
    inject_payload(include_str!("scripts/picker.js"), payload_json)
}

pub(super) fn build_trained_search_js(payload_json: &str) -> String {
    inject_payload(include_str!("scripts/trained_search.js"), payload_json)
}

pub(super) fn build_trained_download_js(payload_json: &str) -> String {
    inject_payload(include_str!("scripts/trained_download.js"), payload_json)
}

pub(super) fn build_vuemotion_search_js(payload_json: &str) -> String {
    inject_payload(include_str!("scripts/vuemotion_search.js"), payload_json)
}

pub(super) fn build_vuemotion_open_patient_js(payload_json: &str) -> String {
    inject_payload(include_str!("scripts/vuemotion_open_patient.js"), payload_json)
}

pub(super) fn build_vuemotion_download_js(payload_json: &str) -> String {
    inject_payload(include_str!("scripts/vuemotion_download.js"), payload_json)
}

pub(super) fn build_auto_login_js(payload_json: &str) -> String {
    inject_payload(include_str!("scripts/auto_login.js"), payload_json)
}
