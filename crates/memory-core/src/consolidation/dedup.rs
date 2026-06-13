use std::collections::HashSet;

pub fn entity_overlap(e1: &[String], e2: &[String]) -> f64 {
    if e1.is_empty() || e2.is_empty() {
        return 0.0;
    }
    let s1: HashSet<&String> = e1.iter().collect();
    let s2: HashSet<&String> = e2.iter().collect();
    let intersection = s1.intersection(&s2).count();
    
    // Overlap is the size of the intersection divided by the size of the smaller set
    intersection as f64 / s1.len().min(s2.len()) as f64
}
