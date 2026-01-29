use crate::state::AppState;
use tv_core::BlockClass;

/// Generate a JSON report of the current analysis.
pub fn export_json(state: &AppState) -> String {
    let mut json = String::from("{\n");

    // File info
    json.push_str(&format!("  \"file\": {{\n"));
    json.push_str(&format!("    \"name\": {:?},\n", state.file_name()));
    json.push_str(&format!("    \"path\": {:?},\n", state.file_path_display()));
    json.push_str(&format!("    \"size\": {}\n", state.file_len()));
    json.push_str("  },\n");

    // Entropy summary
    if let Some(ref entropy) = state.entropy {
        let avg = if entropy.is_empty() {
            0.0
        } else {
            entropy.iter().sum::<f32>() / entropy.len() as f32
        };
        let min = entropy.iter().copied().fold(f32::INFINITY, f32::min);
        let max = entropy.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        json.push_str(&format!("  \"entropy\": {{\n"));
        json.push_str(&format!("    \"blocks\": {},\n", entropy.len()));
        json.push_str(&format!("    \"avg\": {:.4},\n", avg));
        json.push_str(&format!("    \"min\": {:.4},\n", if min.is_finite() { min } else { 0.0 }));
        json.push_str(&format!("    \"max\": {:.4}\n", if max.is_finite() { max } else { 0.0 }));
        json.push_str("  },\n");
    }

    // Classification summary
    if let Some(ref classification) = state.classification {
        let mut counts = [0u32; 5];
        for &c in classification.iter() {
            counts[(c as usize).min(4)] += 1;
        }
        let total = classification.len();
        json.push_str("  \"classification\": {\n");
        json.push_str(&format!("    \"total_blocks\": {},\n", total));
        json.push_str("    \"breakdown\": {\n");
        for (i, &count) in counts.iter().enumerate() {
            let class = BlockClass::from_u8(i as u8);
            let comma = if i < 4 { "," } else { "" };
            json.push_str(&format!("      {:?}: {}{}\n", class.label(), count, comma));
        }
        json.push_str("    }\n");
        json.push_str("  },\n");
    }

    // Signatures
    if let Some(ref sigs) = state.signatures {
        json.push_str("  \"signatures\": [\n");
        for (i, sig) in sigs.iter().enumerate() {
            let comma = if i < sigs.len() - 1 { "," } else { "" };
            let magic_hex: Vec<String> = sig.magic.iter().map(|b| format!("{:02X}", b)).collect();
            json.push_str(&format!(
                "    {{\"offset\": {}, \"name\": {:?}, \"magic\": {:?}}}{}\n",
                sig.offset, sig.name, magic_hex.join(" "), comma
            ));
        }
        json.push_str("  ],\n");
    }

    // Search results
    if let Some(ref results) = state.search.results {
        let pattern_hex = state.search.pattern.as_ref().map(|p| {
            p.iter().map(|b| format!("{:02X}", b)).collect::<Vec<_>>().join(" ")
        }).unwrap_or_default();

        json.push_str("  \"search\": {\n");
        json.push_str(&format!("    \"pattern\": {:?},\n", pattern_hex));
        json.push_str(&format!("    \"matches\": {}\n", results.len()));
        if !results.is_empty() {
            json.pop(); // remove \n
            json.push_str(",\n");
            json.push_str("    \"offsets\": [");
            let offsets: Vec<String> = results.iter().map(|o| o.to_string()).collect();
            json.push_str(&offsets.join(", "));
            json.push_str("]\n");
        }
        json.push_str("  }\n");
    } else {
        // Remove trailing comma from last section
        if json.ends_with(",\n") {
            json.truncate(json.len() - 2);
            json.push('\n');
        }
    }

    json.push_str("}\n");
    json
}

/// Generate a CSV report of search results.
pub fn export_search_csv(state: &AppState) -> String {
    let mut csv = String::from("index,offset_dec,offset_hex\n");
    if let Some(ref results) = state.search.results {
        for (i, &offset) in results.iter().enumerate() {
            csv.push_str(&format!("{},{},0x{:X}\n", i + 1, offset, offset));
        }
    }
    csv
}

/// Generate a CSV report of signatures.
pub fn export_signatures_csv(state: &AppState) -> String {
    let mut csv = String::from("offset_dec,offset_hex,name,magic\n");
    if let Some(ref sigs) = state.signatures {
        for sig in sigs {
            let magic_hex: Vec<String> = sig.magic.iter().map(|b| format!("{:02X}", b)).collect();
            csv.push_str(&format!(
                "{},0x{:X},{},{}\n",
                sig.offset, sig.offset, sig.name, magic_hex.join(" ")
            ));
        }
    }
    csv
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_export_default_state() {
        let state = AppState::default();
        let json = export_json(&state);
        assert!(json.contains("\"file\""));
        assert!(json.contains("\"name\""));
        assert!(json.contains("\"size\": 0"));
    }

    #[test]
    fn csv_search_empty() {
        let state = AppState::default();
        let csv = export_search_csv(&state);
        assert_eq!(csv, "index,offset_dec,offset_hex\n");
    }

    #[test]
    fn csv_signatures_empty() {
        let state = AppState::default();
        let csv = export_signatures_csv(&state);
        assert_eq!(csv, "offset_dec,offset_hex,name,magic\n");
    }
}
