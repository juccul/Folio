//! Strict, side-effect-free dialog validation.
pub(super) fn page_size(content: &str) -> Result<(f32, f32), &'static str> {
    let values = content
        .split(['×', 'x', ',', ' '])
        .filter(|s| !s.is_empty())
        .map(str::parse::<f32>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "Enter width × height, such as 794 × 1123.")?;
    if values.len() != 2
        || values
            .iter()
            .any(|v| !v.is_finite() || !(32.0..=20000.0).contains(v))
    {
        return Err("Width and height must be between 32 and 20,000 points.");
    }
    Ok((values[0], values[1]))
}
pub(super) fn font_size(content: &str) -> Result<f32, &'static str> {
    content
        .trim()
        .parse::<f32>()
        .ok()
        .filter(|v| v.is_finite() && (6.0..=180.0).contains(v))
        .ok_or("Enter a font size between 6 and 180 points.")
}
pub(super) fn crop(content: &str) -> Result<[f32; 4], &'static str> {
    let message = "Use four fractions inside 0–1, e.g. 0.1, 0.1, 0.8, 0.8.";
    let values = content
        .split(',')
        .map(|v| v.trim().parse::<f32>())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| message)?;
    let parts: [f32; 4] = values.try_into().map_err(|_| message)?;
    if parts.iter().any(|v| !v.is_finite())
        || parts[0] < 0.
        || parts[1] < 0.
        || parts[2] <= 0.
        || parts[3] <= 0.
        || parts[0] + parts[2] > 1.
        || parts[1] + parts[3] > 1.
    {
        return Err(message);
    }
    Ok(parts)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dimensions_reject_nonfinite_out_of_range_and_extra_tokens() {
        assert_eq!(page_size("794 × 1123"), Ok((794., 1123.)));
        for input in [
            "NaN x 200",
            "inf, 200",
            "31 200",
            "20001 200",
            "100 100 garbage",
            "100 100 100",
        ] {
            assert!(page_size(input).is_err(), "{input}");
        }
    }
    #[test]
    fn font_and_crop_validation_does_not_silently_drop_values() {
        assert_eq!(font_size(" 20 "), Ok(20.));
        for input in ["NaN", "5", "181", "20 pt"] {
            assert!(font_size(input).is_err());
        }
        assert_eq!(crop("0.1, 0.1, 0.8, 0.8"), Ok([0.1, 0.1, 0.8, 0.8]));
        for input in ["0,0,1,1,garbage", "0,0,NaN,1", "0,0,0,1", "0.5,0,1,1"] {
            assert!(crop(input).is_err());
        }
    }
}
