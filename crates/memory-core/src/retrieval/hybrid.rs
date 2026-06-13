pub fn normalize_bm25(results: &[(String, f32)]) -> Vec<(String, f32)> {
    if results.is_empty() {
        return Vec::new();
    }
    if results.len() == 1 {
        return vec![(results[0].0.clone(), 1.0)];
    }
    let mut min_val = results[0].1;
    let mut max_val = results[0].1;
    for (_, val) in results.iter().skip(1) {
        if *val < min_val { min_val = *val; }
        if *val > max_val { max_val = *val; }
    }
    let range = max_val - min_val;
    results.iter().map(|(id, val)| {
        let norm = if range > 0.0 { (val - min_val) / range } else { 1.0 };
        (id.clone(), norm)
    }).collect()
}
