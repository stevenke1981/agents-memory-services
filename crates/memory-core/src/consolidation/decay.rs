/// 計算 Ebbinghaus 衰減後的保留率
pub fn calculate_retention(
    stability_days: f64, // S: 穩定性係數
    elapsed_days: f64,   // t: 距最後存取天數
) -> f64 {
    if stability_days <= 0.0 {
        return 0.0;
    }
    // R(t) = e^(-t/S)
    (-elapsed_days / stability_days).exp().clamp(0.0, 1.0)
}

/// 每次存取後強化記憶 (stability 增加)
pub fn reinforce_stability(current_stability: f64) -> f64 {
    current_stability * 1.2
}

/// 初始穩定性係數
pub fn initial_stability(importance_score: f64) -> f64 {
    // 重要記憶有更長的穩定週期 (最長 30 天)
    (importance_score * 30.0).max(1.0)
}
