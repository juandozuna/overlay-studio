pub fn parse_color(text: &str) -> Result<[u8; 4], String> {
    let text = text.trim().trim_start_matches('#');
    if !text.is_ascii() || !matches!(text.len(), 6 | 8) {
        return Err("Use #RRGGBB or #RRGGBBAA".into());
    }
    let value =
        u32::from_str_radix(text, 16).map_err(|_| "Enter a hexadecimal color".to_string())?;
    Ok(if text.len() == 6 {
        [(value >> 16) as u8, (value >> 8) as u8, value as u8, 255]
    } else {
        [
            (value >> 24) as u8,
            (value >> 16) as u8,
            (value >> 8) as u8,
            value as u8,
        ]
    })
}
pub fn color_text(color: [u8; 4]) -> String {
    format!(
        "#{:02X}{:02X}{:02X}{:02X}",
        color[0], color[1], color[2], color[3]
    )
}
