use ecow::EcoString;
use indexmap::indexset;
use nu_protocol::Value;

pub fn merge_descriptors(values: &[Value]) -> Vec<EcoString> {
    let mut ret = vec![];
    let mut seen = indexset! {};
    for value in values {
        for desc in value.columns() {
            if !desc.is_empty() && !seen.contains(desc) {
                seen.insert(desc);
                ret.push(desc.clone());
            }
        }
    }
    ret
}
