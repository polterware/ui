//! CSS colors as GPUI [`Rgba`].
//!
//! A scene carries the CSS strings upstream's theme variables and compiled styles write
//! (`#ECECFF`, `rgb(236, 236, 255)`, `transparent`, `currentColor`). The painter resolves them
//! here; `none` is not a color and returns `None`, while an unknown value is left unpainted
//! rather than guessed. Named colors cover the set Mermaid's built-in themes and shape styles
//! use; themes with a palette of their own carry their colors as hex or `rgb()`.

use gpui::Rgba;

#[cfg(test)]
use gpui::rgb;

/// Resolves `css` into a paint color. `current` is what `currentColor` means in this context.
pub(crate) fn css_color(css: &str, current: Rgba) -> Option<Rgba> {
    let css = css.trim();
    if css.is_empty() {
        return None;
    }
    let lower = css.to_ascii_lowercase();
    match lower.as_str() {
        "none" => return None,
        "transparent" => return Some(Rgba::default()),
        "currentcolor" => return Some(current),
        _ => {}
    }
    if let Some(hex) = lower.strip_prefix('#') {
        return hex_color(hex);
    }
    if let Some(arguments) = function_arguments(&lower, "rgb") {
        return rgb_function(arguments, false);
    }
    if let Some(arguments) = function_arguments(&lower, "rgba") {
        return rgb_function(arguments, true);
    }
    if let Some(arguments) = function_arguments(&lower, "hsl") {
        return hsl_function(arguments, false);
    }
    if let Some(arguments) = function_arguments(&lower, "hsla") {
        return hsl_function(arguments, true);
    }
    named_color(&lower)
}

/// Multiplies a color's alpha by `factor`, the group and style opacities.
pub(crate) fn with_alpha(color: Rgba, factor: f64) -> Rgba {
    Rgba {
        a: color.a * factor.clamp(0.0, 1.0) as f32,
        ..color
    }
}

fn hex_color(hex: &str) -> Option<Rgba> {
    let digits: Vec<u32> = hex
        .chars()
        .map(|ch| ch.to_digit(16))
        .collect::<Option<_>>()?;
    let (r, g, b, a) = match digits.as_slice() {
        [r, g, b] => (*r * 17, *g * 17, *b * 17, 255),
        [r, g, b, a] => (*r * 17, *g * 17, *b * 17, *a * 17),
        [r1, r2, g1, g2, b1, b2] => (r1 * 16 + r2, g1 * 16 + g2, b1 * 16 + b2, 255),
        [r1, r2, g1, g2, b1, b2, a1, a2] => {
            (r1 * 16 + r2, g1 * 16 + g2, b1 * 16 + b2, a1 * 16 + a2)
        }
        _ => return None,
    };
    Some(Rgba {
        r: r as f32 / 255.0,
        g: g as f32 / 255.0,
        b: b as f32 / 255.0,
        a: a as f32 / 255.0,
    })
}

fn function_arguments<'a>(css: &'a str, name: &str) -> Option<&'a str> {
    let rest = css.strip_prefix(name)?;
    let rest = rest.trim_start().strip_prefix('(')?;
    rest.strip_suffix(')').map(str::trim)
}

fn components(arguments: &str) -> Vec<&str> {
    arguments
        .split(|ch: char| ch == ',' || ch == '/' || ch.is_whitespace())
        .filter(|part| !part.is_empty())
        .collect()
}

fn rgb_function(arguments: &str, with_alpha: bool) -> Option<Rgba> {
    let parts = components(arguments);
    if parts.len() < 3 || (with_alpha && parts.len() < 4) {
        return None;
    }
    let channel = |part: &str| -> Option<f32> {
        if let Some(percent) = part.strip_suffix('%') {
            Some(percent.parse::<f32>().ok()? / 100.0)
        } else {
            Some(part.parse::<f32>().ok()? / 255.0)
        }
    };
    let alpha = |part: &str| -> Option<f32> {
        if let Some(percent) = part.strip_suffix('%') {
            Some(percent.parse::<f32>().ok()? / 100.0)
        } else {
            part.parse::<f32>().ok()
        }
    };
    Some(Rgba {
        r: channel(parts[0])?.clamp(0.0, 1.0),
        g: channel(parts[1])?.clamp(0.0, 1.0),
        b: channel(parts[2])?.clamp(0.0, 1.0),
        a: if with_alpha {
            alpha(parts[3])?.clamp(0.0, 1.0)
        } else {
            1.0
        },
    })
}

fn hsl_function(arguments: &str, with_alpha: bool) -> Option<Rgba> {
    let parts = components(arguments);
    if parts.len() < 3 || (with_alpha && parts.len() < 4) {
        return None;
    }
    let hue = parts[0]
        .trim_end_matches("deg")
        .parse::<f32>()
        .ok()?
        .rem_euclid(360.0)
        / 360.0;
    let saturation = parts[1].strip_suffix('%')?.parse::<f32>().ok()? / 100.0;
    let lightness = parts[2].strip_suffix('%')?.parse::<f32>().ok()? / 100.0;
    let alpha = if with_alpha {
        if let Some(percent) = parts[3].strip_suffix('%') {
            percent.parse::<f32>().ok()? / 100.0
        } else {
            parts[3].parse::<f32>().ok()?
        }
    } else {
        1.0
    };
    let (r, g, b) = hue_to_rgb(hue, saturation.clamp(0.0, 1.0), lightness.clamp(0.0, 1.0));
    Some(Rgba {
        r,
        g,
        b,
        a: alpha.clamp(0.0, 1.0),
    })
}

fn hue_to_rgb(hue: f32, saturation: f32, lightness: f32) -> (f32, f32, f32) {
    if saturation == 0.0 {
        return (lightness, lightness, lightness);
    }
    let q = if lightness < 0.5 {
        lightness * (1.0 + saturation)
    } else {
        lightness + saturation - lightness * saturation
    };
    let p = 2.0 * lightness - q;
    (
        hue_channel(p, q, hue + 1.0 / 3.0),
        hue_channel(p, q, hue),
        hue_channel(p, q, hue - 1.0 / 3.0),
    )
}

fn hue_channel(p: f32, q: f32, mut t: f32) -> f32 {
    t = t.rem_euclid(1.0);
    if t < 1.0 / 6.0 {
        p + (q - p) * 6.0 * t
    } else if t < 1.0 / 2.0 {
        q
    } else if t < 2.0 / 3.0 {
        p + (q - p) * (2.0 / 3.0 - t) * 6.0
    } else {
        p
    }
}

fn named_color(name: &str) -> Option<Rgba> {
    let (r, g, b) = match name {
        "black" => (0, 0, 0),
        "white" => (255, 255, 255),
        "red" => (255, 0, 0),
        "green" => (0, 128, 0),
        "lime" => (0, 255, 0),
        "blue" => (0, 0, 255),
        "yellow" => (255, 255, 0),
        "orange" => (255, 165, 0),
        "purple" => (128, 0, 128),
        "fuchsia" | "magenta" => (255, 0, 255),
        "aqua" | "cyan" => (0, 255, 255),
        "teal" => (0, 128, 128),
        "navy" => (0, 0, 128),
        "olive" => (128, 128, 0),
        "maroon" => (128, 0, 0),
        "silver" => (192, 192, 192),
        "gray" | "grey" => (128, 128, 128),
        "lightgray" | "lightgrey" => (211, 211, 211),
        "darkgray" | "darkgrey" => (169, 169, 169),
        "pink" => (255, 192, 203),
        "brown" => (165, 42, 42),
        "gold" => (255, 215, 0),
        "violet" => (238, 130, 238),
        "indigo" => (75, 0, 130),
        "beige" => (245, 245, 220),
        "ivory" => (255, 255, 240),
        _ => return None,
    };
    Some(Rgba {
        r: r as f32 / 255.0,
        g: g as f32 / 255.0,
        b: b as f32 / 255.0,
        a: 1.0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn channels(color: Rgba) -> (f32, f32, f32, f32) {
        (color.r, color.g, color.b, color.a)
    }

    #[test]
    fn hex_forms_resolve() {
        assert_eq!(
            channels(css_color("#ECECFF", rgb(0)).unwrap()),
            (236.0 / 255.0, 236.0 / 255.0, 1.0, 1.0)
        );
        assert_eq!(
            channels(css_color("#fff", rgb(0)).unwrap()),
            (1.0, 1.0, 1.0, 1.0)
        );
        assert_eq!(
            channels(css_color("#ff000080", rgb(0)).unwrap()).3,
            128.0 / 255.0
        );
    }

    #[test]
    fn functional_and_named_forms_resolve() {
        let color = css_color("rgb(255, 0, 0)", rgb(0)).unwrap();
        assert_eq!(channels(color), (1.0, 0.0, 0.0, 1.0));
        let color = css_color("rgba(0, 0, 255, 0.5)", rgb(0)).unwrap();
        assert_eq!(channels(color), (0.0, 0.0, 1.0, 0.5));
        let color = css_color("hsl(120, 100%, 50%)", rgb(0)).unwrap();
        assert!((color.g - 1.0).abs() < 1e-5);
        assert_eq!(
            channels(css_color("white", rgb(0)).unwrap()),
            (1.0, 1.0, 1.0, 1.0)
        );
    }

    #[test]
    fn none_and_unknown_values_paint_nothing() {
        assert!(css_color("none", rgb(0)).is_none());
        assert!(css_color("not-a-color", rgb(0)).is_none());
        assert_eq!(css_color("currentColor", rgb(12)).unwrap(), rgb(12));
        assert_eq!(channels(css_color("transparent", rgb(0)).unwrap()).3, 0.0);
    }

    #[test]
    fn opacity_multiplies_the_alpha() {
        let color = css_color("#000000", rgb(0)).unwrap();
        assert_eq!(with_alpha(color, 0.25).a, 0.25);
        assert_eq!(with_alpha(color, 2.0).a, 1.0);
    }
}
