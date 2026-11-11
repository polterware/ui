//! Mermaid's five built-in themes: `default`, `dark`, `forest`, `neutral` and `base`.
//!
//! Ported from `themes/theme-*.js` and `theme-helpers.js`. Each theme builds the same
//! variable names upstream does, in the same order, with the derived values computed by
//! khroma's color math ([`color`]). User overrides are applied twice, as upstream's
//! `calculate` does: once before the derived values are computed, so a base color feeds the
//! rest, and once after, so an overridden derived value is the one that survives. The one
//! deliberate difference upstream has kept is kept here too: the `quadrantPointFill`
//! expression falls to the ternary before `||`, so its `lighten`/`darken` gets no amount and
//! the value carries `NaN` unless it was overridden.
//!
//! `themeCSS` is a stylesheet string rather than a generated variable, so [`crate::config`]
//! keeps it as text; only `themeVariables` overrides reach a theme here.

use std::fmt;

use serde_json::{Map, Value, json};

use self::color::ColorError;

/// A theme variable that cannot be computed, such as a color that does not parse.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThemeError {
    message: String,
}

impl ThemeError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for ThemeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ThemeError {}

impl From<ColorError> for ThemeError {
    fn from(error: ColorError) -> Self {
        Self::new(error.to_string())
    }
}

/// The five themes this port implements, by the name a configuration uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Theme {
    Default,
    Dark,
    Forest,
    Neutral,
    Base,
}

impl Theme {
    /// Every implemented theme, in upstream's registry order filtered to these five.
    pub const ALL: [Theme; 5] = [
        Theme::Base,
        Theme::Dark,
        Theme::Default,
        Theme::Forest,
        Theme::Neutral,
    ];

    /// The name a configuration's `theme` uses.
    pub fn name(self) -> &'static str {
        match self {
            Theme::Default => "default",
            Theme::Dark => "dark",
            Theme::Forest => "forest",
            Theme::Neutral => "neutral",
            Theme::Base => "base",
        }
    }

    /// The theme for a configuration name, or `None` for a name this port does not
    /// implement (`neo`, `redux`, …).
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "default" => Some(Theme::Default),
            "dark" => Some(Theme::Dark),
            "forest" => Some(Theme::Forest),
            "neutral" => Some(Theme::Neutral),
            "base" => Some(Theme::Base),
            _ => None,
        }
    }

    /// Builds the theme's variables with user overrides on top, as `getThemeVariables`
    /// does.
    pub fn variables(self, overrides: &Map<String, Value>) -> Result<ThemeVariables, ThemeError> {
        let mut vars = match self {
            Theme::Default => default_theme::constructor()?,
            Theme::Dark => dark_theme::constructor(),
            Theme::Forest => forest_theme::constructor()?,
            Theme::Neutral => neutral_theme::constructor()?,
            Theme::Base => base_theme::constructor(),
        };
        if self == Theme::Default {
            // theme-default.js marks "calculated" values in its constructor, which runs
            // updateColors() once; calculate() clears those values before applying overrides
            // and then calls updateColors() again, so the color scale darkens twice.
            vars.values.retain(|_, value| value != &json!("calculated"));
        }
        for (key, value) in overrides {
            vars.set(key, value.clone());
        }
        match self {
            Theme::Default => default_theme::update_colors(&mut vars)?,
            Theme::Dark => dark_theme::update_colors(&mut vars)?,
            Theme::Forest => forest_theme::update_colors(&mut vars)?,
            Theme::Neutral => neutral_theme::update_colors(&mut vars)?,
            Theme::Base => base_theme::update_colors(&mut vars)?,
        }
        for (key, value) in overrides {
            let generated = vars.values.remove(key);
            vars.set(key, apply_override(generated, value));
        }
        // theme-base.js's calculate(): an explicit nodeBorder paints, so the neo gradient
        // (which would otherwise draw the stroke) turns off unless useGradient was asked for.
        if self == Theme::Base
            && overrides.contains_key("nodeBorder")
            && !overrides.contains_key("useGradient")
        {
            vars.set("useGradient", json!(false));
        }
        Ok(ThemeVariables {
            values: vars.values,
        })
    }
}

/// `theme-helpers.js`'s `applyOverride`: an object-valued variable keeps every generated key
/// and takes the user's keys on top; everything else is replaced.
fn apply_override(generated: Option<Value>, over: &Value) -> Value {
    match (generated, over) {
        (Some(Value::Object(mut generated)), Value::Object(over)) => {
            for (key, value) in over {
                generated.insert(key.clone(), value.clone());
            }
            Value::Object(generated)
        }
        _ => over.clone(),
    }
}

/// The variables a theme computes, by name.
///
/// A map rather than a struct: upstream builds a class by assignment, a user override may
/// add any key, and object-valued variables such as `xyChart` take a partial override on top
/// of the generated keys. Typed accessors cover the common reads.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ThemeVariables {
    values: Map<String, Value>,
}

impl ThemeVariables {
    /// A variable's value.
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.values.get(key)
    }

    /// A color variable, when it is a string.
    pub fn color(&self, key: &str) -> Option<&str> {
        self.values.get(key).and_then(Value::as_str)
    }

    /// A numeric variable, when it is a number.
    pub fn number(&self, key: &str) -> Option<f64> {
        self.values.get(key).and_then(Value::as_f64)
    }

    /// A boolean variable, when it is a boolean.
    pub fn boolean(&self, key: &str) -> Option<bool> {
        self.values.get(key).and_then(Value::as_bool)
    }

    /// One of the twelve `cScale{n}` colors.
    pub fn c_scale(&self, index: usize) -> Option<&str> {
        self.color(&format!("cScale{index}"))
    }

    /// The variables as a JSON object.
    pub fn to_json(&self) -> Value {
        Value::Object(self.values.clone())
    }

    /// The variables as a map.
    pub fn as_map(&self) -> &Map<String, Value> {
        &self.values
    }

    /// The variables as their map, by value.
    pub fn into_map(self) -> Map<String, Value> {
        self.values
    }
}

/// The builder the theme functions write into, transcribing `this.x` reads and writes.
struct Vars {
    values: Map<String, Value>,
}

impl Vars {
    fn new() -> Self {
        Self { values: Map::new() }
    }

    fn set(&mut self, key: &str, value: Value) {
        self.values.insert(key.to_string(), value);
    }

    fn set_text(&mut self, key: &str, value: &str) {
        self.set(key, Value::String(value.to_string()));
    }

    fn value(&self, key: &str) -> Value {
        self.values.get(key).cloned().unwrap_or(Value::Null)
    }

    fn text(&self, key: &str) -> Option<&str> {
        self.values.get(key).and_then(Value::as_str)
    }

    fn truthy(&self, key: &str) -> bool {
        self.values.get(key).is_some_and(truthy)
    }

    fn dark_mode(&self) -> bool {
        self.truthy("darkMode")
    }

    fn color(&self, key: &str) -> Result<&str, ThemeError> {
        match self.values.get(key) {
            Some(Value::String(value)) => Ok(value),
            Some(_) => Err(ThemeError::new(format!(
                "theme variable \"{key}\" is not a color"
            ))),
            None => Err(ThemeError::new(format!(
                "theme variable \"{key}\" is missing"
            ))),
        }
    }

    fn adjust(&self, key: &str, channels: &[(&str, f64)]) -> Result<String, ThemeError> {
        Ok(color::adjust(self.color(key)?, channels)?)
    }

    fn lighten(&self, key: &str, amount: f64) -> Result<String, ThemeError> {
        Ok(color::lighten(self.color(key)?, amount)?)
    }

    fn darken(&self, key: &str, amount: f64) -> Result<String, ThemeError> {
        Ok(color::darken(self.color(key)?, amount)?)
    }

    fn invert(&self, key: &str) -> Result<String, ThemeError> {
        Ok(color::invert(self.color(key)?, 100.0)?)
    }

    fn lighten_color(&self, value: &str, amount: f64) -> Result<String, ThemeError> {
        Ok(color::lighten(value, amount)?)
    }

    fn darken_color(&self, value: &str, amount: f64) -> Result<String, ThemeError> {
        Ok(color::darken(value, amount)?)
    }

    fn is_dark(&self, key: &str) -> Result<bool, ThemeError> {
        Ok(color::is_dark(self.color(key)?)?)
    }

    /// `mkBorder(col, darkMode)`.
    fn mk_border(&self, key: &str) -> Result<String, ThemeError> {
        let lightness = if self.dark_mode() { 10.0 } else { -10.0 };
        self.adjust(key, &[("s", -40.0), ("l", lightness)])
    }

    /// `this.obj?.key || fallback` where the fallback is another variable.
    fn nested_or(&self, object: &str, key: &str, fallback: &str) -> Value {
        match self.values.get(object).and_then(|value| value.get(key)) {
            Some(value) if truthy(value) => value.clone(),
            _ => self.value(fallback),
        }
    }

    /// `this.obj?.key || literal`.
    fn nested_or_else(&self, object: &str, key: &str, fallback: fn() -> Value) -> Value {
        match self.values.get(object).and_then(|value| value.get(key)) {
            Some(value) if truthy(value) => value.clone(),
            _ => fallback(),
        }
    }
}

fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_f64().is_some_and(|n| n != 0.0 && !n.is_nan()),
        Value::String(value) => !value.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

/// Builds a JSON object, leaving out a value that upstream would have left `undefined`.
fn object(entries: Vec<(&str, Value)>) -> Value {
    let mut map = Map::new();
    for (key, value) in entries {
        if !value.is_null() {
            map.insert(key.to_string(), value);
        }
    }
    Value::Object(map)
}

/// `this.x = this.x || value`.
macro_rules! default_to {
    ($vars:ident, $key:expr, $value:expr) => {
        if !$vars.truthy($key) {
            let value = $value;
            $vars.set($key, value);
        }
    };
}

/// `this.x = this.x || this.y`.
macro_rules! default_from {
    ($vars:ident, $key:expr, $source:expr) => {
        if !$vars.truthy($key) {
            let value = $vars.value($source);
            $vars.set($key, value);
        }
    };
}

/// `this.x = this.x ?? value`.
macro_rules! default_nullish {
    ($vars:ident, $key:expr, $value:expr) => {
        match $vars.values.get($key) {
            None | Some(Value::Null) => {
                let value = $value;
                $vars.set($key, value);
            }
            Some(_) => {}
        }
    };
}

/// The `quadrantPointFill` expression: `this.quadrantPointFill || isDark(...) ?
/// lighten(...) : darken(...)` binds the ternary before `||`, and it passes no amount, so a
/// light quadrant fill gets `NaN` lightness. Ported as is so the value matches upstream.
fn default_quadrant_point_fill(vars: &mut Vars) -> Result<(), ThemeError> {
    if vars.truthy("quadrantPointFill") || vars.is_dark("quadrant1Fill")? {
        default_to!(
            vars,
            "quadrantPointFill",
            json!(vars.lighten("quadrant1Fill", f64::NAN)?)
        );
    } else {
        default_to!(
            vars,
            "quadrantPointFill",
            json!(vars.darken("quadrant1Fill", f64::NAN)?)
        );
    }
    Ok(())
}

/// The part of [khroma](https://github.com/fabiospampinato/khroma) (MIT, Copyright (c)
/// Fabio Spampinato) that Mermaid's five built-in themes use: `lighten`, `darken`, `adjust`,
/// `invert`, `rgba` and `isDark`, with khroma's parse, stringify and channel conversion rules
/// so the generated values are the same strings upstream produces, including
/// `hsl(...)`/`rgba(...)` output and the NaN a missing adjustment produces.
pub mod color {
    // The part of khroma that the themes use; see the module documentation above.
    // `lighten`, `darken`, `adjust`, `invert`, `rgba` and `isDark`.

    use std::fmt;

    /// A color khroma cannot parse, or a channel that cannot be changed once the other color
    /// space has been touched.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct ColorError {
        message: String,
    }

    impl ColorError {
        fn new(message: impl Into<String>) -> Self {
            Self {
                message: message.into(),
            }
        }
    }

    impl fmt::Display for ColorError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str(&self.message)
        }
    }

    impl std::error::Error for ColorError {}

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum ColorType {
        All,
        Rgb,
        Hsl,
    }

    #[derive(Clone, Debug, Default)]
    struct Data {
        r: Option<f64>,
        g: Option<f64>,
        b: Option<f64>,
        h: Option<f64>,
        s: Option<f64>,
        l: Option<f64>,
        a: f64,
    }

    #[derive(Clone, Debug)]
    struct Channels {
        data: Data,
        original: Option<String>,
        changed: bool,
        kind: ColorType,
    }

    impl Channels {
        fn new(data: Data, original: Option<String>) -> Self {
            Self {
                data,
                original,
                changed: false,
                kind: ColorType::All,
            }
        }

        fn parse(color: &str) -> Result<Self, ColorError> {
            if let Some(channels) = parse_hex(color) {
                return Ok(channels);
            }
            if let Some(channels) = parse_rgb(color) {
                return Ok(channels);
            }
            if let Some(channels) = parse_hsl(color) {
                return Ok(channels);
            }
            if let Some(channels) = parse_keyword(color) {
                return Ok(channels);
            }
            Err(ColorError::new(format!(
                "Unsupported color format: \"{color}\""
            )))
        }

        fn get(&mut self, channel: &str) -> Result<f64, ColorError> {
            match channel {
                "r" => Ok(self.r()),
                "g" => Ok(self.g()),
                "b" => Ok(self.b()),
                "h" => Ok(self.h()),
                "s" => Ok(self.s()),
                "l" => Ok(self.l()),
                "a" => Ok(self.data.a),
                other => Err(ColorError::new(format!("Unknown channel: {other}"))),
            }
        }

        fn set(&mut self, channel: &str, value: f64) -> Result<(), ColorError> {
            match channel {
                "r" => {
                    self.set_type(ColorType::Rgb)?;
                    self.data.r = Some(value);
                }
                "g" => {
                    self.set_type(ColorType::Rgb)?;
                    self.data.g = Some(value);
                }
                "b" => {
                    self.set_type(ColorType::Rgb)?;
                    self.data.b = Some(value);
                }
                "h" => {
                    self.set_type(ColorType::Hsl)?;
                    self.data.h = Some(value);
                }
                "s" => {
                    self.set_type(ColorType::Hsl)?;
                    self.data.s = Some(value);
                }
                "l" => {
                    self.set_type(ColorType::Hsl)?;
                    self.data.l = Some(value);
                }
                "a" => self.data.a = value,
                other => return Err(ColorError::new(format!("Unknown channel: {other}"))),
            }
            self.changed = true;
            Ok(())
        }

        fn set_type(&mut self, kind: ColorType) -> Result<(), ColorError> {
            if self.kind != ColorType::All && self.kind != kind {
                return Err(ColorError::new(
                    "Cannot change both RGB and HSL channels at the same time",
                ));
            }
            self.kind = kind;
            Ok(())
        }

        fn ensure_hsl(&mut self) {
            if self.data.h.is_none() {
                self.data.h = Some(rgb2hsl(&self.data, "h"));
            }
            if self.data.s.is_none() {
                self.data.s = Some(rgb2hsl(&self.data, "s"));
            }
            if self.data.l.is_none() {
                self.data.l = Some(rgb2hsl(&self.data, "l"));
            }
        }

        fn ensure_rgb(&mut self) {
            if self.data.r.is_none() {
                self.data.r = Some(hsl2rgb(&self.data, "r"));
            }
            if self.data.g.is_none() {
                self.data.g = Some(hsl2rgb(&self.data, "g"));
            }
            if self.data.b.is_none() {
                self.data.b = Some(hsl2rgb(&self.data, "b"));
            }
        }

        fn r(&mut self) -> f64 {
            if self.kind != ColorType::Hsl
                && let Some(r) = self.data.r
            {
                return r;
            }
            self.ensure_hsl();
            hsl2rgb(&self.data, "r")
        }

        fn g(&mut self) -> f64 {
            if self.kind != ColorType::Hsl
                && let Some(g) = self.data.g
            {
                return g;
            }
            self.ensure_hsl();
            hsl2rgb(&self.data, "g")
        }

        fn b(&mut self) -> f64 {
            if self.kind != ColorType::Hsl
                && let Some(b) = self.data.b
            {
                return b;
            }
            self.ensure_hsl();
            hsl2rgb(&self.data, "b")
        }

        fn h(&mut self) -> f64 {
            if self.kind != ColorType::Rgb
                && let Some(h) = self.data.h
            {
                return h;
            }
            self.ensure_rgb();
            rgb2hsl(&self.data, "h")
        }

        fn s(&mut self) -> f64 {
            if self.kind != ColorType::Rgb
                && let Some(s) = self.data.s
            {
                return s;
            }
            self.ensure_rgb();
            rgb2hsl(&self.data, "s")
        }

        fn l(&mut self) -> f64 {
            if self.kind != ColorType::Rgb
                && let Some(l) = self.data.l
            {
                return l;
            }
            self.ensure_rgb();
            rgb2hsl(&self.data, "l")
        }

        fn stringify(&mut self) -> String {
            if !self.changed
                && let Some(original) = &self.original
            {
                return original.clone();
            }
            if self.kind == ColorType::Hsl || self.data.r.is_none() {
                return self.stringify_hsl();
            }
            let (r, g, b, a) = (self.r(), self.g(), self.b(), self.data.a);
            if a < 1.0 {
                format!(
                    "rgba({}, {}, {}, {})",
                    number(round(r)),
                    number(round(g)),
                    number(round(b)),
                    number(round(a))
                )
            } else if !is_integer(r) || !is_integer(g) || !is_integer(b) {
                format!(
                    "rgb({}, {}, {})",
                    number(round(r)),
                    number(round(g)),
                    number(round(b))
                )
            } else {
                format!(
                    "#{:02x}{:02x}{:02x}",
                    js_round(r) as u8,
                    js_round(g) as u8,
                    js_round(b) as u8
                )
            }
        }

        fn stringify_hsl(&mut self) -> String {
            let (h, s, l, a) = (self.h(), self.s(), self.l(), self.data.a);
            if a < 1.0 {
                format!(
                    "hsla({}, {}%, {}%, {})",
                    number(round(h)),
                    number(round(s)),
                    number(round(l)),
                    number(a)
                )
            } else {
                format!(
                    "hsl({}, {}%, {}%)",
                    number(round(h)),
                    number(round(s)),
                    number(round(l))
                )
            }
        }
    }

    fn is_integer(value: f64) -> bool {
        value.is_finite() && value.fract() == 0.0
    }

    /// `Math.round`: halves go up, and negative zero prints as zero.
    fn js_round(value: f64) -> f64 {
        (value + 0.5).floor()
    }

    /// `_.lang.round`: ten decimal places.
    fn round(value: f64) -> f64 {
        js_round(value * 10000000000.0) / 10000000000.0
    }

    /// A JS number as `String()` prints it.
    fn number(value: f64) -> String {
        if value == 0.0 {
            return "0".to_string();
        }
        let magnitude = value.abs();
        if !(1e-6..1e21).contains(&magnitude) {
            let formatted = format!("{value:e}");
            if let Some((mantissa, exponent)) = formatted.split_once('e') {
                let exponent: i32 = exponent.parse().unwrap_or(0);
                let sign = if exponent >= 0 { "+" } else { "-" };
                return format!("{mantissa}e{sign}{}", exponent.abs());
            }
            return formatted;
        }
        format!("{value}")
    }

    fn clamp_h(value: f64) -> f64 {
        value % 360.0
    }

    fn clamp_s(value: f64) -> f64 {
        value.clamp(0.0, 100.0)
    }

    fn clamp_l(value: f64) -> f64 {
        clamp_s(value)
    }

    fn clamp_rgb(value: f64) -> f64 {
        value.clamp(0.0, 255.0)
    }

    fn clamp_alpha(value: f64) -> f64 {
        value.clamp(0.0, 1.0)
    }

    fn clamp_channel(channel: &str, value: f64) -> f64 {
        match channel {
            "r" | "g" | "b" => clamp_rgb(value),
            "h" => clamp_h(value),
            "s" => clamp_s(value),
            "l" => clamp_l(value),
            "a" => clamp_alpha(value),
            _ => value,
        }
    }

    fn hue2rgb(p: f64, q: f64, mut t: f64) -> f64 {
        if t < 0.0 {
            t += 1.0;
        }
        if t > 1.0 {
            t -= 1.0;
        }
        if t < 1.0 / 6.0 {
            return p + (q - p) * 6.0 * t;
        }
        if t < 1.0 / 2.0 {
            return q;
        }
        if t < 2.0 / 3.0 {
            return p + (q - p) * (2.0 / 3.0 - t) * 6.0;
        }
        p
    }

    fn hsl2rgb(data: &Data, channel: &str) -> f64 {
        let h = data.h.unwrap_or(0.0);
        let s = data.s.unwrap_or(0.0);
        let l = data.l.unwrap_or(0.0);
        if s == 0.0 {
            return l * 2.55;
        }
        let h = h / 360.0;
        let s = s / 100.0;
        let l = l / 100.0;
        let q = if l < 0.5 {
            l * (1.0 + s)
        } else {
            (l + s) - (l * s)
        };
        let p = 2.0 * l - q;
        match channel {
            "r" => hue2rgb(p, q, h + 1.0 / 3.0) * 255.0,
            "g" => hue2rgb(p, q, h) * 255.0,
            _ => hue2rgb(p, q, h - 1.0 / 3.0) * 255.0,
        }
    }

    fn rgb2hsl(data: &Data, channel: &str) -> f64 {
        let r = data.r.unwrap_or(0.0) / 255.0;
        let g = data.g.unwrap_or(0.0) / 255.0;
        let b = data.b.unwrap_or(0.0) / 255.0;
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let l = (max + min) / 2.0;
        if channel == "l" {
            return l * 100.0;
        }
        if max == min {
            return 0.0;
        }
        let d = max - min;
        let s = if l > 0.5 {
            d / (2.0 - max - min)
        } else {
            d / (max + min)
        };
        if channel == "s" {
            return s * 100.0;
        }
        if max == r {
            ((g - b) / d + if g < b { 6.0 } else { 0.0 }) * 60.0
        } else if max == g {
            ((b - r) / d + 2.0) * 60.0
        } else {
            ((r - g) / d + 4.0) * 60.0
        }
    }

    fn to_linear(channel: f64) -> f64 {
        let n = channel / 255.0;
        if channel > 0.03928 {
            ((n + 0.055) / 1.055).powf(2.4)
        } else {
            n / 12.92
        }
    }

    fn luminance(color: &str) -> Result<f64, ColorError> {
        let mut channels = Channels::parse(color)?;
        let (r, g, b) = (channels.r(), channels.g(), channels.b());
        Ok(round(
            0.2126 * to_linear(r) + 0.7152 * to_linear(g) + 0.0722 * to_linear(b),
        ))
    }

    fn parse_hex(color: &str) -> Option<Channels> {
        let hex = color.strip_prefix('#')?;
        if !matches!(hex.len(), 3 | 4 | 6 | 8) || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        let dec = u64::from_str_radix(hex, 16).ok()?;
        let length = hex.len() as u32;
        let has_alpha = length.is_multiple_of(4);
        let full_length = length > 4;
        let multiplier = if full_length { 1.0 } else { 17.0 };
        let bits = if full_length { 8 } else { 4 };
        let offset: i32 = if has_alpha { 0 } else { -1 };
        let mask: u64 = if full_length { 255 } else { 15 };
        let channel = |index: u32| -> f64 {
            ((dec >> (bits * (index as i32 + offset) as u32)) & mask) as f64 * multiplier
        };
        let alpha = if has_alpha {
            (dec & mask) as f64 * multiplier / 255.0
        } else {
            1.0
        };
        Some(Channels::new(
            Data {
                r: Some(channel(3)),
                g: Some(channel(2)),
                b: Some(channel(1)),
                a: alpha,
                ..Data::default()
            },
            Some(color.to_string()),
        ))
    }

    fn tokenize(inner: &str) -> Vec<&str> {
        inner
            .split(|c: char| c == ',' || c == '/' || c.is_whitespace())
            .filter(|token| !token.is_empty())
            .collect()
    }

    fn parse_channel_token(token: &str, scale: f64) -> Option<f64> {
        if let Some(percent) = token.strip_suffix('%') {
            Some(percent.parse::<f64>().ok()? * scale)
        } else {
            token.parse::<f64>().ok()
        }
    }

    fn parse_rgb(color: &str) -> Option<Channels> {
        let lower = color.to_ascii_lowercase();
        let inner = lower
            .strip_prefix("rgba(")
            .or_else(|| lower.strip_prefix("rgb("))?
            .strip_suffix(')')?;
        let tokens = tokenize(inner);
        if !(3..=4).contains(&tokens.len()) {
            return None;
        }
        let r = parse_channel_token(tokens[0], 2.55)?;
        let g = parse_channel_token(tokens[1], 2.55)?;
        let b = parse_channel_token(tokens[2], 2.55)?;
        let a = match tokens.get(3) {
            Some(token) => parse_channel_token(token, 0.01)?,
            None => 1.0,
        };
        Some(Channels::new(
            Data {
                r: Some(clamp_rgb(r)),
                g: Some(clamp_rgb(g)),
                b: Some(clamp_rgb(b)),
                a: clamp_alpha(a),
                ..Data::default()
            },
            Some(color.to_string()),
        ))
    }

    fn parse_hue(token: &str) -> f64 {
        let lower = token.to_ascii_lowercase();
        for (unit, factor) in [
            ("grad", 0.9),
            ("rad", 180.0 / std::f64::consts::PI),
            ("turn", 360.0),
        ] {
            if let Some(value) = lower.strip_suffix(unit) {
                return clamp_h(value.parse::<f64>().unwrap_or(f64::NAN) * factor);
            }
        }
        if let Some(value) = lower.strip_suffix("deg") {
            return clamp_h(value.parse::<f64>().unwrap_or(f64::NAN));
        }
        clamp_h(lower.parse::<f64>().unwrap_or(f64::NAN))
    }

    fn parse_hsl(color: &str) -> Option<Channels> {
        let lower = color.to_ascii_lowercase();
        let inner = lower
            .strip_prefix("hsla(")
            .or_else(|| lower.strip_prefix("hsl("))?
            .strip_suffix(')')?;
        let tokens = tokenize(inner);
        if !(3..=4).contains(&tokens.len()) {
            return None;
        }
        let h = parse_hue(tokens[0]);
        let s = tokens[1].strip_suffix('%')?.parse::<f64>().ok()?;
        let l = tokens[2].strip_suffix('%')?.parse::<f64>().ok()?;
        let a = match tokens.get(3) {
            Some(token) => clamp_alpha(parse_channel_token(token, 0.01)?),
            None => 1.0,
        };
        Some(Channels::new(
            Data {
                h: Some(h),
                s: Some(clamp_s(s)),
                l: Some(clamp_l(l)),
                a,
                ..Data::default()
            },
            Some(color.to_string()),
        ))
    }

    fn parse_keyword(color: &str) -> Option<Channels> {
        let hex = keyword_hex(color.to_ascii_lowercase().as_str())?;
        let mut channels = parse_hex(hex)?;
        channels.original = Some(color.to_string());
        Some(channels)
    }

    fn keyword_hex(name: &str) -> Option<&'static str> {
        let hex = match name {
            "aliceblue" => "#f0f8ff",
            "antiquewhite" => "#faebd7",
            "aqua" => "#00ffff",
            "aquamarine" => "#7fffd4",
            "azure" => "#f0ffff",
            "beige" => "#f5f5dc",
            "bisque" => "#ffe4c4",
            "black" => "#000000",
            "blanchedalmond" => "#ffebcd",
            "blue" => "#0000ff",
            "blueviolet" => "#8a2be2",
            "brown" => "#a52a2a",
            "burlywood" => "#deb887",
            "cadetblue" => "#5f9ea0",
            "chartreuse" => "#7fff00",
            "chocolate" => "#d2691e",
            "coral" => "#ff7f50",
            "cornflowerblue" => "#6495ed",
            "cornsilk" => "#fff8dc",
            "crimson" => "#dc143c",
            "cyanaqua" => "#00ffff",
            "darkblue" => "#00008b",
            "darkcyan" => "#008b8b",
            "darkgoldenrod" => "#b8860b",
            "darkgray" => "#a9a9a9",
            "darkgreen" => "#006400",
            "darkgrey" => "#a9a9a9",
            "darkkhaki" => "#bdb76b",
            "darkmagenta" => "#8b008b",
            "darkolivegreen" => "#556b2f",
            "darkorange" => "#ff8c00",
            "darkorchid" => "#9932cc",
            "darkred" => "#8b0000",
            "darksalmon" => "#e9967a",
            "darkseagreen" => "#8fbc8f",
            "darkslateblue" => "#483d8b",
            "darkslategray" => "#2f4f4f",
            "darkslategrey" => "#2f4f4f",
            "darkturquoise" => "#00ced1",
            "darkviolet" => "#9400d3",
            "deeppink" => "#ff1493",
            "deepskyblue" => "#00bfff",
            "dimgray" => "#696969",
            "dimgrey" => "#696969",
            "dodgerblue" => "#1e90ff",
            "firebrick" => "#b22222",
            "floralwhite" => "#fffaf0",
            "forestgreen" => "#228b22",
            "fuchsia" => "#ff00ff",
            "gainsboro" => "#dcdcdc",
            "ghostwhite" => "#f8f8ff",
            "gold" => "#ffd700",
            "goldenrod" => "#daa520",
            "gray" => "#808080",
            "green" => "#008000",
            "greenyellow" => "#adff2f",
            "grey" => "#808080",
            "honeydew" => "#f0fff0",
            "hotpink" => "#ff69b4",
            "indianred" => "#cd5c5c",
            "indigo" => "#4b0082",
            "ivory" => "#fffff0",
            "khaki" => "#f0e68c",
            "lavender" => "#e6e6fa",
            "lavenderblush" => "#fff0f5",
            "lawngreen" => "#7cfc00",
            "lemonchiffon" => "#fffacd",
            "lightblue" => "#add8e6",
            "lightcoral" => "#f08080",
            "lightcyan" => "#e0ffff",
            "lightgoldenrodyellow" => "#fafad2",
            "lightgray" => "#d3d3d3",
            "lightgreen" => "#90ee90",
            "lightgrey" => "#d3d3d3",
            "lightpink" => "#ffb6c1",
            "lightsalmon" => "#ffa07a",
            "lightseagreen" => "#20b2aa",
            "lightskyblue" => "#87cefa",
            "lightslategray" => "#778899",
            "lightslategrey" => "#778899",
            "lightsteelblue" => "#b0c4de",
            "lightyellow" => "#ffffe0",
            "lime" => "#00ff00",
            "limegreen" => "#32cd32",
            "linen" => "#faf0e6",
            "magenta" => "#ff00ff",
            "maroon" => "#800000",
            "mediumaquamarine" => "#66cdaa",
            "mediumblue" => "#0000cd",
            "mediumorchid" => "#ba55d3",
            "mediumpurple" => "#9370db",
            "mediumseagreen" => "#3cb371",
            "mediumslateblue" => "#7b68ee",
            "mediumspringgreen" => "#00fa9a",
            "mediumturquoise" => "#48d1cc",
            "mediumvioletred" => "#c71585",
            "midnightblue" => "#191970",
            "mintcream" => "#f5fffa",
            "mistyrose" => "#ffe4e1",
            "moccasin" => "#ffe4b5",
            "navajowhite" => "#ffdead",
            "navy" => "#000080",
            "oldlace" => "#fdf5e6",
            "olive" => "#808000",
            "olivedrab" => "#6b8e23",
            "orange" => "#ffa500",
            "orangered" => "#ff4500",
            "orchid" => "#da70d6",
            "palegoldenrod" => "#eee8aa",
            "palegreen" => "#98fb98",
            "paleturquoise" => "#afeeee",
            "palevioletred" => "#db7093",
            "papayawhip" => "#ffefd5",
            "peachpuff" => "#ffdab9",
            "peru" => "#cd853f",
            "pink" => "#ffc0cb",
            "plum" => "#dda0dd",
            "powderblue" => "#b0e0e6",
            "purple" => "#800080",
            "rebeccapurple" => "#663399",
            "red" => "#ff0000",
            "rosybrown" => "#bc8f8f",
            "royalblue" => "#4169e1",
            "saddlebrown" => "#8b4513",
            "salmon" => "#fa8072",
            "sandybrown" => "#f4a460",
            "seagreen" => "#2e8b57",
            "seashell" => "#fff5ee",
            "sienna" => "#a0522d",
            "silver" => "#c0c0c0",
            "skyblue" => "#87ceeb",
            "slateblue" => "#6a5acd",
            "slategray" => "#708090",
            "slategrey" => "#708090",
            "snow" => "#fffafa",
            "springgreen" => "#00ff7f",
            "tan" => "#d2b48c",
            "teal" => "#008080",
            "thistle" => "#d8bfd8",
            "transparent" => "#00000000",
            "turquoise" => "#40e0d0",
            "violet" => "#ee82ee",
            "wheat" => "#f5deb3",
            "white" => "#ffffff",
            "whitesmoke" => "#f5f5f5",
            "yellow" => "#ffff00",
            "yellowgreen" => "#9acd32",
            _ => return None,
        };
        Some(hex)
    }

    /// `lighten(color, amount)`: adds `amount` percentage points of lightness.
    pub fn lighten(color: &str, amount: f64) -> Result<String, ColorError> {
        adjust_channel(color, "l", amount)
    }

    /// `darken(color, amount)`: removes `amount` percentage points of lightness.
    pub fn darken(color: &str, amount: f64) -> Result<String, ColorError> {
        adjust_channel(color, "l", -amount)
    }

    fn adjust_channel(color: &str, channel: &str, amount: f64) -> Result<String, ColorError> {
        let mut channels = Channels::parse(color)?;
        let current = channels.get(channel)?;
        let next = clamp_channel(channel, current + amount);
        if current != next {
            channels.set(channel, next)?;
        }
        Ok(channels.stringify())
    }

    /// `adjust(color, { h, s, l, r, g, b, a })`: adds to each named channel, skipping zeros the
    /// way upstream's `if (!channels[c]) continue` does.
    pub fn adjust(color: &str, channels: &[(&str, f64)]) -> Result<String, ColorError> {
        let mut parsed = Channels::parse(color)?;
        let mut changes = Vec::new();
        for (channel, amount) in channels {
            if *amount == 0.0 || amount.is_nan() {
                continue;
            }
            let current = parsed.get(channel)?;
            changes.push((*channel, current + amount));
        }
        for (channel, value) in changes {
            parsed.set(channel, clamp_channel(channel, value))?;
        }
        Ok(parsed.stringify())
    }

    /// `invert(color, weight)`, with the Sass `mix` behind it.
    pub fn invert(color: &str, weight: f64) -> Result<String, ColorError> {
        let mut inverse = Channels::parse(color)?;
        let r = 255.0 - inverse.r();
        let g = 255.0 - inverse.g();
        let b = 255.0 - inverse.b();
        inverse.set("r", r)?;
        inverse.set("g", g)?;
        inverse.set("b", b)?;
        mix_with(&mut inverse, color, weight)
    }

    fn mix_with(first: &mut Channels, second: &str, weight: f64) -> Result<String, ColorError> {
        let mut other = Channels::parse(second)?;
        let (r1, g1, b1, a1) = (first.r(), first.g(), first.b(), first.data.a);
        let (r2, g2, b2, a2) = (other.r(), other.g(), other.b(), other.data.a);
        let weight_scale = weight / 100.0;
        let weight_normalized = (weight_scale * 2.0) - 1.0;
        let alpha_delta = a1 - a2;
        let combined = if weight_normalized * alpha_delta == -1.0 {
            weight_normalized
        } else {
            (weight_normalized + alpha_delta) / (1.0 + weight_normalized * alpha_delta)
        };
        let weight1 = (combined + 1.0) / 2.0;
        let weight2 = 1.0 - weight1;
        let r = (r1 * weight1) + (r2 * weight2);
        let g = (g1 * weight1) + (g2 * weight2);
        let b = (b1 * weight1) + (b2 * weight2);
        let a = (a1 * weight_scale) + (a2 * (1.0 - weight_scale));
        Ok(rgba(r, g, b, a))
    }

    /// `rgba(r, g, b, a)`: clamps and stringifies the way khroma does.
    pub fn rgba(r: f64, g: f64, b: f64, a: f64) -> String {
        let mut channels = Channels::new(
            Data {
                r: Some(clamp_rgb(r)),
                g: Some(clamp_rgb(g)),
                b: Some(clamp_rgb(b)),
                a: clamp_alpha(a),
                ..Data::default()
            },
            None,
        );
        channels.stringify()
    }

    /// `isDark(color)`: luminance below half.
    pub fn is_dark(color: &str) -> Result<bool, ColorError> {
        let luminance = luminance(color)?;
        Ok(luminance < 0.5 || luminance.is_nan())
    }
}

mod base_theme {
    use super::*;

    const OLD_ATTRIBUTE_ODD: &str = "#ffffff";
    const OLD_ATTRIBUTE_EVEN: &str = "#f2f2f2";

    pub(super) fn constructor() -> Vars {
        let mut vars = Vars::new();
        vars.set_text("background", "#f4f4f4");
        vars.set_text("primaryColor", "#fff4dd");
        vars.set_text("noteBkgColor", "#fff5ad");
        vars.set_text("noteTextColor", "#333");
        vars.set("THEME_COLOR_LIMIT", json!(12));
        vars.set("radius", json!(5));
        vars.set("strokeWidth", json!(1));
        vars.set_text("fontFamily", "\"trebuchet ms\", verdana, arial, sans-serif");
        vars.set_text("fontSize", "16px");
        vars.set("useGradient", json!(true));
        vars.set_text(
            "dropShadow",
            "drop-shadow( 1px 2px 2px rgba(185,185,185,1))",
        );
        vars
    }

    pub(super) fn update_colors(vars: &mut Vars) -> Result<(), ThemeError> {
        default_to!(
            vars,
            "primaryTextColor",
            json!(if vars.dark_mode() { "#eee" } else { "#333" })
        );
        default_to!(
            vars,
            "secondaryColor",
            json!(vars.adjust("primaryColor", &[("h", -120.0)])?)
        );
        default_to!(
            vars,
            "tertiaryColor",
            json!(vars.adjust("primaryColor", &[("h", 180.0), ("l", 5.0)])?)
        );

        default_to!(
            vars,
            "primaryBorderColor",
            json!(vars.mk_border("primaryColor")?)
        );
        default_to!(
            vars,
            "secondaryBorderColor",
            json!(vars.mk_border("secondaryColor")?)
        );
        default_to!(
            vars,
            "tertiaryBorderColor",
            json!(vars.mk_border("tertiaryColor")?)
        );
        default_to!(
            vars,
            "noteBorderColor",
            json!(vars.mk_border("noteBkgColor")?)
        );
        default_to!(vars, "noteBkgColor", json!("#fff5ad"));
        default_to!(vars, "noteTextColor", json!("#333"));

        default_to!(
            vars,
            "secondaryTextColor",
            json!(vars.invert("secondaryColor")?)
        );
        default_to!(
            vars,
            "tertiaryTextColor",
            json!(vars.invert("tertiaryColor")?)
        );
        default_to!(vars, "lineColor", json!(vars.invert("background")?));
        default_to!(vars, "arrowheadColor", json!(vars.invert("background")?));
        default_from!(vars, "textColor", "primaryTextColor");
        default_from!(vars, "border2", "tertiaryBorderColor");

        default_from!(vars, "nodeBkg", "primaryColor");
        default_from!(vars, "mainBkg", "primaryColor");
        default_from!(vars, "nodeBorder", "primaryBorderColor");
        default_from!(vars, "clusterBkg", "tertiaryColor");
        default_from!(vars, "clusterBorder", "tertiaryBorderColor");
        default_from!(vars, "defaultLinkColor", "lineColor");
        default_from!(vars, "titleColor", "tertiaryTextColor");
        default_to!(
            vars,
            "edgeLabelBackground",
            if vars.dark_mode() {
                json!(vars.darken("secondaryColor", 30.0)?)
            } else {
                vars.value("secondaryColor")
            }
        );

        default_from!(vars, "flowContainerStroke", "secondaryBorderColor");
        default_from!(vars, "nodeTextColor", "primaryTextColor");

        default_from!(vars, "actorBorder", "primaryBorderColor");
        default_from!(vars, "actorBkg", "mainBkg");
        default_from!(vars, "actorTextColor", "primaryTextColor");
        default_from!(vars, "actorLineColor", "actorBorder");
        default_from!(vars, "labelBoxBkgColor", "actorBkg");
        default_from!(vars, "signalColor", "textColor");
        default_from!(vars, "signalTextColor", "textColor");
        default_from!(vars, "labelBoxBorderColor", "actorBorder");
        default_from!(vars, "labelTextColor", "actorTextColor");
        default_from!(vars, "loopTextColor", "actorTextColor");
        default_to!(
            vars,
            "activationBorderColor",
            json!(vars.darken("secondaryColor", 10.0)?)
        );
        default_from!(vars, "activationBkgColor", "secondaryColor");
        default_to!(
            vars,
            "sequenceNumberColor",
            json!(vars.invert("lineColor")?)
        );
        default_from!(vars, "rectBkgColor", "tertiaryColor");

        default_from!(vars, "sectionBkgColor", "tertiaryColor");
        default_to!(vars, "altSectionBkgColor", json!("white"));
        default_from!(vars, "sectionBkgColor", "secondaryColor");
        default_from!(vars, "sectionBkgColor2", "primaryColor");
        default_to!(vars, "excludeBkgColor", json!("#eeeeee"));
        default_from!(vars, "taskBorderColor", "primaryBorderColor");
        default_from!(vars, "taskBkgColor", "primaryColor");
        default_from!(vars, "activeTaskBorderColor", "primaryColor");
        default_to!(
            vars,
            "activeTaskBkgColor",
            json!(vars.lighten("primaryColor", 23.0)?)
        );
        default_to!(vars, "gridColor", json!("lightgrey"));
        default_to!(vars, "doneTaskBkgColor", json!("lightgrey"));
        default_to!(vars, "doneTaskBorderColor", json!("grey"));
        default_to!(vars, "critBorderColor", json!("#ff8888"));
        default_to!(vars, "critBkgColor", json!("red"));
        default_to!(vars, "todayLineColor", json!("red"));
        default_to!(vars, "vertLineColor", json!("navy"));
        default_from!(vars, "taskTextColor", "textColor");
        default_from!(vars, "taskTextOutsideColor", "textColor");
        default_from!(vars, "taskTextLightColor", "textColor");
        default_from!(vars, "taskTextColor", "primaryTextColor");
        default_from!(vars, "taskTextDarkColor", "textColor");
        default_to!(vars, "taskTextClickableColor", json!("#003163"));

        default_to!(vars, "noteFontWeight", json!("normal"));
        default_to!(vars, "fontWeight", json!("normal"));

        default_from!(vars, "personBorder", "primaryBorderColor");
        default_from!(vars, "personBkg", "mainBkg");

        if vars.dark_mode() {
            default_to!(vars, "rowOdd", json!(vars.darken("mainBkg", 5.0)?));
            default_to!(vars, "rowEven", json!(vars.darken("mainBkg", 10.0)?));
        } else {
            default_to!(vars, "rowOdd", json!(vars.lighten("mainBkg", 75.0)?));
            default_to!(vars, "rowEven", json!(vars.lighten("mainBkg", 5.0)?));
        }

        default_from!(vars, "transitionColor", "lineColor");
        default_from!(vars, "transitionLabelColor", "textColor");
        default_to!(
            vars,
            "stateLabelColor",
            if vars.truthy("stateBkg") {
                vars.value("stateBkg")
            } else {
                vars.value("primaryTextColor")
            }
        );
        default_from!(vars, "stateBkg", "mainBkg");
        default_from!(vars, "labelBackgroundColor", "stateBkg");
        default_from!(vars, "compositeBackground", "background");
        default_from!(vars, "altBackground", "tertiaryColor");
        default_from!(vars, "compositeTitleBackground", "mainBkg");
        default_from!(vars, "compositeBorder", "nodeBorder");
        vars.set("innerEndBackground", vars.value("nodeBorder"));
        default_from!(vars, "errorBkgColor", "tertiaryColor");
        default_from!(vars, "errorTextColor", "tertiaryTextColor");
        default_from!(vars, "transitionColor", "lineColor");
        vars.set("specialStateColor", vars.value("lineColor"));

        let limit = color_limit(vars);
        for i in 0..limit {
            let key = format!("cScale{i}");
            default_to!(
                vars,
                &key,
                match i {
                    0 => vars.value("primaryColor"),
                    1 => vars.value("secondaryColor"),
                    2 => vars.value("tertiaryColor"),
                    3 => json!(vars.adjust("primaryColor", &[("h", 30.0)])?),
                    4 => json!(vars.adjust("primaryColor", &[("h", 60.0)])?),
                    5 => json!(vars.adjust("primaryColor", &[("h", 90.0)])?),
                    6 => json!(vars.adjust("primaryColor", &[("h", 120.0)])?),
                    7 => json!(vars.adjust("primaryColor", &[("h", 150.0)])?),
                    8 => json!(vars.adjust("primaryColor", &[("h", 210.0), ("l", 150.0)])?),
                    9 => json!(vars.adjust("primaryColor", &[("h", 270.0)])?),
                    10 => json!(vars.adjust("primaryColor", &[("h", 300.0)])?),
                    _ => json!(vars.adjust("primaryColor", &[("h", 330.0)])?),
                }
            );
        }
        for i in 0..limit {
            let key = format!("cScale{i}");
            let amount = if vars.dark_mode() { 75.0 } else { 25.0 };
            let value = json!(vars.darken(&key, amount)?);
            vars.set(&key, value);
        }
        for i in 0..limit {
            let key = format!("cScaleInv{i}");
            let scale = format!("cScale{i}");
            default_to!(vars, &key, json!(vars.invert(&scale)?));
        }
        for i in 0..limit {
            let key = format!("cScalePeer{i}");
            let scale = format!("cScale{i}");
            let value = if vars.dark_mode() {
                json!(vars.lighten(&scale, 10.0)?)
            } else {
                json!(vars.darken(&scale, 10.0)?)
            };
            default_to!(vars, &key, value);
        }

        default_from!(vars, "scaleLabelColor", "labelTextColor");
        for i in 0..limit {
            let key = format!("cScaleLabel{i}");
            default_from!(vars, &key, "scaleLabelColor");
        }

        let multiplier = if vars.dark_mode() { -4.0 } else { -1.0 };
        for i in 0..5 {
            let key = format!("surface{i}");
            default_to!(
                vars,
                &key,
                json!(vars.adjust(
                    "mainBkg",
                    &[
                        ("h", 180.0),
                        ("s", -15.0),
                        ("l", multiplier * (5.0 + i as f64 * 3.0)),
                    ]
                )?)
            );
            let key = format!("surfacePeer{i}");
            default_to!(
                vars,
                &key,
                json!(vars.adjust(
                    "mainBkg",
                    &[
                        ("h", 180.0),
                        ("s", -15.0),
                        ("l", multiplier * (8.0 + i as f64 * 3.0)),
                    ]
                )?)
            );
        }

        default_from!(vars, "classText", "textColor");

        default_from!(vars, "fillType0", "primaryColor");
        default_from!(vars, "fillType1", "secondaryColor");
        default_to!(
            vars,
            "fillType2",
            json!(vars.adjust("primaryColor", &[("h", 64.0)])?)
        );
        default_to!(
            vars,
            "fillType3",
            json!(vars.adjust("secondaryColor", &[("h", 64.0)])?)
        );
        default_to!(
            vars,
            "fillType4",
            json!(vars.adjust("primaryColor", &[("h", -64.0)])?)
        );
        default_to!(
            vars,
            "fillType5",
            json!(vars.adjust("secondaryColor", &[("h", -64.0)])?)
        );
        default_to!(
            vars,
            "fillType6",
            json!(vars.adjust("primaryColor", &[("h", 128.0)])?)
        );
        default_to!(
            vars,
            "fillType7",
            json!(vars.adjust("secondaryColor", &[("h", 128.0)])?)
        );

        default_from!(vars, "pie1", "primaryColor");
        default_from!(vars, "pie2", "secondaryColor");
        default_from!(vars, "pie3", "tertiaryColor");
        default_to!(
            vars,
            "pie4",
            json!(vars.adjust("primaryColor", &[("l", -10.0)])?)
        );
        default_to!(
            vars,
            "pie5",
            json!(vars.adjust("secondaryColor", &[("l", -10.0)])?)
        );
        default_to!(
            vars,
            "pie6",
            json!(vars.adjust("tertiaryColor", &[("l", -10.0)])?)
        );
        default_to!(
            vars,
            "pie7",
            json!(vars.adjust("primaryColor", &[("h", 60.0), ("l", -10.0)])?)
        );
        default_to!(
            vars,
            "pie8",
            json!(vars.adjust("primaryColor", &[("h", -60.0), ("l", -10.0)])?)
        );
        default_to!(
            vars,
            "pie9",
            json!(vars.adjust("primaryColor", &[("h", 120.0), ("l", 0.0)])?)
        );
        default_to!(
            vars,
            "pie10",
            json!(vars.adjust("primaryColor", &[("h", 60.0), ("l", -20.0)])?)
        );
        default_to!(
            vars,
            "pie11",
            json!(vars.adjust("primaryColor", &[("h", -60.0), ("l", -20.0)])?)
        );
        default_to!(
            vars,
            "pie12",
            json!(vars.adjust("primaryColor", &[("h", 120.0), ("l", -10.0)])?)
        );
        default_to!(vars, "pieTitleTextSize", json!("25px"));
        default_from!(vars, "pieTitleTextColor", "taskTextDarkColor");
        default_to!(vars, "pieSectionTextSize", json!("17px"));
        default_from!(vars, "pieSectionTextColor", "textColor");
        default_to!(vars, "pieLegendTextSize", json!("17px"));
        default_from!(vars, "pieLegendTextColor", "taskTextDarkColor");
        default_to!(vars, "pieStrokeColor", json!("black"));
        default_to!(vars, "pieStrokeWidth", json!("2px"));
        default_to!(vars, "pieOuterStrokeWidth", json!("2px"));
        default_to!(vars, "pieOuterStrokeColor", json!("black"));
        default_to!(vars, "pieOpacity", json!("0.7"));

        default_nullish!(
            vars,
            "venn1",
            json!(vars.adjust("primaryColor", &[("l", -30.0)])?)
        );
        default_nullish!(
            vars,
            "venn2",
            json!(vars.adjust("secondaryColor", &[("l", -30.0)])?)
        );
        default_nullish!(
            vars,
            "venn3",
            json!(vars.adjust("tertiaryColor", &[("l", -30.0)])?)
        );
        default_nullish!(
            vars,
            "venn4",
            json!(vars.adjust("primaryColor", &[("h", 60.0), ("l", -30.0)])?)
        );
        default_nullish!(
            vars,
            "venn5",
            json!(vars.adjust("primaryColor", &[("h", -60.0), ("l", -30.0)])?)
        );
        default_nullish!(
            vars,
            "venn6",
            json!(vars.adjust("secondaryColor", &[("h", 60.0), ("l", -30.0)])?)
        );
        default_nullish!(
            vars,
            "venn7",
            json!(vars.adjust("primaryColor", &[("h", 120.0), ("l", -30.0)])?)
        );
        default_nullish!(
            vars,
            "venn8",
            json!(vars.adjust("secondaryColor", &[("h", 120.0), ("l", -30.0)])?)
        );
        default_nullish!(vars, "vennTitleTextColor", vars.value("titleColor"));
        default_nullish!(vars, "vennSetTextColor", vars.value("textColor"));

        vars.set(
            "cynefin",
            object(vec![
                (
                    "domainFontSize",
                    vars.nested_or_else("cynefin", "domainFontSize", || json!(16)),
                ),
                (
                    "itemFontSize",
                    vars.nested_or_else("cynefin", "itemFontSize", || json!(12)),
                ),
                (
                    "boundaryColor",
                    vars.nested_or("cynefin", "boundaryColor", "lineColor"),
                ),
                (
                    "boundaryWidth",
                    vars.nested_or_else("cynefin", "boundaryWidth", || json!(2)),
                ),
                (
                    "cliffColor",
                    vars.nested_or_else("cynefin", "cliffColor", || json!("#8B0000")),
                ),
                (
                    "cliffWidth",
                    vars.nested_or_else("cynefin", "cliffWidth", || json!(4)),
                ),
                (
                    "arrowColor",
                    vars.nested_or("cynefin", "arrowColor", "lineColor"),
                ),
                (
                    "arrowWidth",
                    vars.nested_or_else("cynefin", "arrowWidth", || json!(2)),
                ),
                (
                    "complexBg",
                    vars.nested_or_else("cynefin", "complexBg", || json!("#E8F5E9")),
                ),
                (
                    "complicatedBg",
                    vars.nested_or_else("cynefin", "complicatedBg", || json!("#E3F2FD")),
                ),
                (
                    "chaoticBg",
                    vars.nested_or_else("cynefin", "chaoticBg", || json!("#FBE9E7")),
                ),
                (
                    "clearBg",
                    vars.nested_or_else("cynefin", "clearBg", || json!("#FFF8E1")),
                ),
                (
                    "confusionBg",
                    vars.nested_or_else("cynefin", "confusionBg", || json!("#F3E5F5")),
                ),
                (
                    "textColor",
                    vars.nested_or("cynefin", "textColor", "textColor"),
                ),
                (
                    "labelColor",
                    vars.nested_or("cynefin", "labelColor", "primaryTextColor"),
                ),
            ]),
        );

        vars.set(
            "radar",
            object(vec![
                (
                    "axisColor",
                    vars.nested_or("radar", "axisColor", "lineColor"),
                ),
                (
                    "axisStrokeWidth",
                    vars.nested_or_else("radar", "axisStrokeWidth", || json!(2)),
                ),
                (
                    "axisLabelFontSize",
                    vars.nested_or_else("radar", "axisLabelFontSize", || json!(12)),
                ),
                (
                    "curveOpacity",
                    vars.nested_or_else("radar", "curveOpacity", || json!(0.5)),
                ),
                (
                    "curveStrokeWidth",
                    vars.nested_or_else("radar", "curveStrokeWidth", || json!(2)),
                ),
                (
                    "graticuleColor",
                    vars.nested_or_else("radar", "graticuleColor", || json!("#DEDEDE")),
                ),
                (
                    "graticuleStrokeWidth",
                    vars.nested_or_else("radar", "graticuleStrokeWidth", || json!(1)),
                ),
                (
                    "graticuleOpacity",
                    vars.nested_or_else("radar", "graticuleOpacity", || json!(0.3)),
                ),
                (
                    "legendBoxSize",
                    vars.nested_or_else("radar", "legendBoxSize", || json!(12)),
                ),
                (
                    "legendFontSize",
                    vars.nested_or_else("radar", "legendFontSize", || json!(12)),
                ),
            ]),
        );

        default_to!(vars, "wardleyEvolutionColor", json!("#dc3545"));
        vars.set(
            "wardley",
            object(vec![
                (
                    "backgroundColor",
                    vars.nested_or("wardley", "backgroundColor", "background"),
                ),
                (
                    "axisColor",
                    vars.nested_or("wardley", "axisColor", "lineColor"),
                ),
                (
                    "axisTextColor",
                    vars.nested_or("wardley", "axisTextColor", "primaryTextColor"),
                ),
                (
                    "gridColor",
                    vars.nested_or("wardley", "gridColor", "gridColor"),
                ),
                (
                    "componentFill",
                    vars.nested_or("wardley", "componentFill", "background"),
                ),
                (
                    "componentStroke",
                    vars.nested_or("wardley", "componentStroke", "lineColor"),
                ),
                (
                    "componentLabelColor",
                    vars.nested_or("wardley", "componentLabelColor", "primaryTextColor"),
                ),
                (
                    "linkStroke",
                    vars.nested_or("wardley", "linkStroke", "lineColor"),
                ),
                (
                    "evolutionStroke",
                    vars.nested_or("wardley", "evolutionStroke", "wardleyEvolutionColor"),
                ),
                (
                    "annotationStroke",
                    vars.nested_or("wardley", "annotationStroke", "lineColor"),
                ),
                (
                    "annotationTextColor",
                    vars.nested_or("wardley", "annotationTextColor", "primaryTextColor"),
                ),
                (
                    "annotationFill",
                    vars.nested_or("wardley", "annotationFill", "background"),
                ),
            ]),
        );

        default_to!(vars, "archEdgeColor", json!("#777"));
        default_to!(vars, "archEdgeArrowColor", json!("#777"));
        default_to!(vars, "archEdgeWidth", json!("3"));
        default_to!(vars, "archGroupBorderColor", json!("#000"));
        default_to!(vars, "archGroupBorderWidth", json!("2px"));

        default_from!(vars, "quadrant1Fill", "primaryColor");
        default_to!(
            vars,
            "quadrant2Fill",
            json!(vars.adjust("primaryColor", &[("r", 5.0), ("g", 5.0), ("b", 5.0)])?)
        );
        default_to!(
            vars,
            "quadrant3Fill",
            json!(vars.adjust("primaryColor", &[("r", 10.0), ("g", 10.0), ("b", 10.0)])?)
        );
        default_to!(
            vars,
            "quadrant4Fill",
            json!(vars.adjust("primaryColor", &[("r", 15.0), ("g", 15.0), ("b", 15.0)])?)
        );
        default_from!(vars, "quadrant1TextFill", "primaryTextColor");
        default_to!(
            vars,
            "quadrant2TextFill",
            json!(vars.adjust("primaryTextColor", &[("r", -5.0), ("g", -5.0), ("b", -5.0)])?)
        );
        default_to!(
            vars,
            "quadrant3TextFill",
            json!(vars.adjust(
                "primaryTextColor",
                &[("r", -10.0), ("g", -10.0), ("b", -10.0)]
            )?)
        );
        default_to!(
            vars,
            "quadrant4TextFill",
            json!(vars.adjust(
                "primaryTextColor",
                &[("r", -15.0), ("g", -15.0), ("b", -15.0)]
            )?)
        );
        default_quadrant_point_fill(vars)?;
        default_from!(vars, "quadrantPointTextFill", "primaryTextColor");
        default_from!(vars, "quadrantXAxisTextFill", "primaryTextColor");
        default_from!(vars, "quadrantYAxisTextFill", "primaryTextColor");
        default_from!(
            vars,
            "quadrantInternalBorderStrokeFill",
            "primaryBorderColor"
        );
        default_from!(
            vars,
            "quadrantExternalBorderStrokeFill",
            "primaryBorderColor"
        );
        default_from!(vars, "quadrantTitleFill", "primaryTextColor");

        vars.set(
            "xyChart",
            object(vec![
                (
                    "backgroundColor",
                    vars.nested_or("xyChart", "backgroundColor", "background"),
                ),
                (
                    "titleColor",
                    vars.nested_or("xyChart", "titleColor", "primaryTextColor"),
                ),
                (
                    "dataLabelColor",
                    vars.nested_or("xyChart", "dataLabelColor", "primaryTextColor"),
                ),
                (
                    "legendTextColor",
                    vars.nested_or("xyChart", "legendTextColor", "primaryTextColor"),
                ),
                (
                    "xAxisTitleColor",
                    vars.nested_or("xyChart", "xAxisTitleColor", "primaryTextColor"),
                ),
                (
                    "xAxisLabelColor",
                    vars.nested_or("xyChart", "xAxisLabelColor", "primaryTextColor"),
                ),
                (
                    "xAxisTickColor",
                    vars.nested_or("xyChart", "xAxisTickColor", "primaryTextColor"),
                ),
                (
                    "xAxisLineColor",
                    vars.nested_or("xyChart", "xAxisLineColor", "primaryTextColor"),
                ),
                (
                    "yAxisTitleColor",
                    vars.nested_or("xyChart", "yAxisTitleColor", "primaryTextColor"),
                ),
                (
                    "yAxisLabelColor",
                    vars.nested_or("xyChart", "yAxisLabelColor", "primaryTextColor"),
                ),
                (
                    "yAxisTickColor",
                    vars.nested_or("xyChart", "yAxisTickColor", "primaryTextColor"),
                ),
                (
                    "yAxisLineColor",
                    vars.nested_or("xyChart", "yAxisLineColor", "primaryTextColor"),
                ),
                (
                    "plotColorPalette",
                    vars.nested_or_else("xyChart", "plotColorPalette", || {
                        json!(
                            "#FFF4DD,#FFD8B1,#FFA07A,#ECEFF1,#D6DBDF,#C3E0A8,#FFB6A4,#FFD74D,#738FA7,#FFFFF0"
                        )
                    }),
                ),
            ]),
        );

        default_from!(vars, "requirementBackground", "primaryColor");
        default_from!(vars, "requirementBorderColor", "primaryBorderColor");
        default_to!(vars, "requirementBorderSize", json!("1"));
        default_from!(vars, "requirementTextColor", "primaryTextColor");
        default_from!(vars, "relationColor", "lineColor");
        default_to!(
            vars,
            "relationLabelBackground",
            if vars.dark_mode() {
                json!(vars.darken("secondaryColor", 30.0)?)
            } else {
                vars.value("secondaryColor")
            }
        );
        default_from!(vars, "relationLabelColor", "actorTextColor");

        default_from!(vars, "git0", "primaryColor");
        default_from!(vars, "git1", "secondaryColor");
        default_from!(vars, "git2", "tertiaryColor");
        default_to!(
            vars,
            "git3",
            json!(vars.adjust("primaryColor", &[("h", -30.0)])?)
        );
        default_to!(
            vars,
            "git4",
            json!(vars.adjust("primaryColor", &[("h", -60.0)])?)
        );
        default_to!(
            vars,
            "git5",
            json!(vars.adjust("primaryColor", &[("h", -90.0)])?)
        );
        default_to!(
            vars,
            "git6",
            json!(vars.adjust("primaryColor", &[("h", 60.0)])?)
        );
        default_to!(
            vars,
            "git7",
            json!(vars.adjust("primaryColor", &[("h", 120.0)])?)
        );
        for i in 0..8 {
            let key = format!("git{i}");
            let amount = 25.0;
            let value = if vars.dark_mode() {
                json!(vars.lighten(&key, amount)?)
            } else {
                json!(vars.darken(&key, amount)?)
            };
            vars.set(&key, value);
        }
        for i in 0..8 {
            let key = format!("gitInv{i}");
            let source = format!("git{i}");
            default_to!(vars, &key, json!(vars.invert(&source)?));
        }
        default_to!(
            vars,
            "branchLabelColor",
            json!(if vars.dark_mode() {
                "black".to_string()
            } else {
                vars.color("labelTextColor")?.to_string()
            })
        );
        for i in 0..8 {
            let key = format!("gitBranchLabel{i}");
            default_from!(vars, &key, "branchLabelColor");
        }

        default_from!(vars, "tagLabelColor", "primaryTextColor");
        default_from!(vars, "tagLabelBackground", "primaryColor");
        default_to!(
            vars,
            "tagLabelBorder",
            if vars.truthy("tagBorder") {
                vars.value("tagBorder")
            } else {
                vars.value("primaryBorderColor")
            }
        );
        default_to!(vars, "tagLabelFontSize", json!("10px"));
        default_from!(vars, "commitLabelColor", "secondaryTextColor");
        default_from!(vars, "commitLabelBackground", "secondaryColor");
        default_to!(vars, "commitLabelFontSize", json!("10px"));

        default_to!(vars, "emUiFill", json!("white"));
        default_to!(vars, "emUiStroke", json!("#dbdada"));
        default_to!(vars, "emProcessorFill", json!("#edb3f6"));
        default_to!(vars, "emProcessorStroke", json!("#b88cbf"));
        default_to!(vars, "emReadModelFill", json!("#d3f1a2"));
        default_to!(vars, "emReadModelStroke", json!("#a3b732"));
        default_to!(vars, "emCommandFill", json!("#bcd6fe"));
        default_to!(vars, "emCommandStroke", json!("#679ac3"));
        default_to!(vars, "emEventFill", json!("#ffb778"));
        default_to!(vars, "emEventStroke", json!("#c19a0f"));
        default_to!(vars, "emSwimlaneBackgroundOdd", json!("rgb(250,250,250)"));
        default_to!(
            vars,
            "emSwimlaneBackgroundStroke",
            json!("rgb(240,240,240)")
        );
        default_from!(vars, "emArrowhead", "lineColor");
        default_from!(vars, "emRelationStroke", "lineColor");

        default_to!(
            vars,
            "attributeBackgroundColorOdd",
            json!(OLD_ATTRIBUTE_ODD)
        );
        default_to!(
            vars,
            "attributeBackgroundColorEven",
            json!(OLD_ATTRIBUTE_EVEN)
        );

        vars.set("gradientStart", vars.value("primaryBorderColor"));
        vars.set("gradientStop", vars.value("secondaryBorderColor"));
        Ok(())
    }

    fn color_limit(vars: &Vars) -> usize {
        vars.values
            .get("THEME_COLOR_LIMIT")
            .and_then(Value::as_f64)
            .unwrap_or(12.0) as usize
    }
}
mod default_theme {
    use super::*;

    const OLD_ATTRIBUTE_ODD: &str = "#ffffff";
    const OLD_ATTRIBUTE_EVEN: &str = "#f2f2f2";

    pub(super) fn constructor() -> Result<Vars, ThemeError> {
        let mut vars = Vars::new();
        vars.set_text("background", "#f4f4f4");
        vars.set_text("primaryColor", "#ECECFF");
        vars.set_text("secondaryColor", "#ffffde");
        vars.set_text(
            "tertiaryColor",
            &color::adjust("#ECECFF", &[("h", -160.0)])?,
        );
        let primary_border = mk_border("#ECECFF", false)?;
        let secondary_border = mk_border("#ffffde", false)?;
        vars.set_text("primaryBorderColor", &primary_border);
        vars.set_text("secondaryBorderColor", &secondary_border);
        vars.set_text(
            "tertiaryBorderColor",
            &mk_border(&color::adjust("#ECECFF", &[("h", -160.0)])?, false)?,
        );
        vars.set_text("primaryTextColor", &color::invert("#ECECFF", 100.0)?);
        vars.set_text("secondaryTextColor", &color::invert("#ffffde", 100.0)?);
        vars.set_text(
            "tertiaryTextColor",
            &color::invert(&color::adjust("#ECECFF", &[("h", -160.0)])?, 100.0)?,
        );
        vars.set_text("lineColor", &color::invert("#f4f4f4", 100.0)?);
        vars.set_text("textColor", &color::invert("#f4f4f4", 100.0)?);

        vars.set_text("background", "white");
        vars.set_text("mainBkg", "#ECECFF");
        vars.set_text("secondBkg", "#ffffde");
        vars.set_text("lineColor", "#333333");
        vars.set_text("border1", "#9370DB");
        vars.set_text("primaryBorderColor", &mk_border("#ECECFF", false)?);
        vars.set_text("border2", "#aaaa33");
        vars.set_text("arrowheadColor", "#333333");
        vars.set_text("fontFamily", "\"trebuchet ms\", verdana, arial, sans-serif");
        vars.set_text("fontSize", "16px");
        vars.set_text("labelBackground", "rgba(232,232,232, 0.8)");
        vars.set_text("textColor", "#333");
        vars.set("THEME_COLOR_LIMIT", json!(12));
        vars.set("radius", json!(5));
        vars.set("strokeWidth", json!(1));

        vars.set_text("actorTextColor", "black");
        vars.set_text("labelTextColor", "calculated");
        vars.set_text("rowOdd", "calculated");
        vars.set_text("rowEven", "calculated");
        vars.set_text("noteBkgColor", "#fff5ad");
        vars.set_text("activationBorderColor", "#666");
        vars.set_text("activationBkgColor", "#f4f4f4");
        vars.set_text("sequenceNumberColor", "white");
        vars.set_text("clusterBkg", "#FBFBFF");

        vars.set_text("sectionBkgColor", &color::rgba(102.0, 102.0, 255.0, 0.49));
        vars.set_text("altSectionBkgColor", "white");
        vars.set_text("sectionBkgColor2", "#fff400");
        vars.set_text("taskBorderColor", "#534fbc");
        vars.set_text("taskBkgColor", "#8a90dd");
        vars.set_text("taskTextLightColor", "white");
        vars.set_text("taskTextDarkColor", "black");
        vars.set_text("taskTextClickableColor", "#003163");
        vars.set_text("excludeBkgColor", "#eeeeee");
        vars.set_text("activeTaskBorderColor", "#534fbc");
        vars.set_text("activeTaskBkgColor", "#bfc7ff");
        vars.set_text("gridColor", "lightgrey");
        vars.set_text("doneTaskBkgColor", "lightgrey");
        vars.set_text("doneTaskBorderColor", "grey");
        vars.set_text("critBorderColor", "#ff8888");
        vars.set_text("critBkgColor", "red");
        vars.set_text("todayLineColor", "red");
        vars.set_text("vertLineColor", "navy");

        vars.set_text("noteFontWeight", "normal");
        vars.set_text("fontWeight", "normal");

        vars.set_text("personBorder", &primary_border);
        vars.set_text("personBkg", "#ECECFF");

        vars.set_text("archEdgeWidth", "3");
        vars.set_text("archGroupBorderColor", &primary_border);
        vars.set_text("archGroupBorderWidth", "2px");

        vars.set_text("labelColor", "black");
        vars.set_text("errorBkgColor", "#552222");
        vars.set_text("errorTextColor", "#552222");

        vars.set("useGradient", json!(false));
        vars.set_text("gradientStart", &primary_border);
        vars.set_text("gradientStop", &secondary_border);
        vars.set_text(
            "dropShadow",
            "drop-shadow(1px 2px 2px rgba(185, 185, 185, 1))",
        );

        update_colors(&mut vars)?;
        Ok(vars)
    }

    pub(super) fn update_colors(vars: &mut Vars) -> Result<(), ThemeError> {
        let limit = color_limit(vars);
        for i in 0..limit {
            let key = format!("cScale{i}");
            default_to!(
                vars,
                &key,
                match i {
                    0 => vars.value("primaryColor"),
                    1 => vars.value("secondaryColor"),
                    2 => vars.value("tertiaryColor"),
                    3 => json!(vars.adjust("primaryColor", &[("h", 30.0)])?),
                    4 => json!(vars.adjust("primaryColor", &[("h", 60.0)])?),
                    5 => json!(vars.adjust("primaryColor", &[("h", 90.0)])?),
                    6 => json!(vars.adjust("primaryColor", &[("h", 120.0)])?),
                    7 => json!(vars.adjust("primaryColor", &[("h", 150.0)])?),
                    8 => json!(vars.adjust("primaryColor", &[("h", 210.0)])?),
                    9 => json!(vars.adjust("primaryColor", &[("h", 270.0)])?),
                    10 => json!(vars.adjust("primaryColor", &[("h", 300.0)])?),
                    _ => json!(vars.adjust("primaryColor", &[("h", 330.0)])?),
                }
            );
        }
        default_to!(
            vars,
            "cScalePeer1",
            json!(vars.darken("secondaryColor", 45.0)?)
        );
        default_to!(
            vars,
            "cScalePeer2",
            json!(vars.darken("tertiaryColor", 40.0)?)
        );
        for i in 0..limit {
            let scale = format!("cScale{i}");
            let value = json!(vars.darken(&scale, 10.0)?);
            vars.set(&scale, value);
            let peer = format!("cScalePeer{i}");
            default_to!(vars, &peer, json!(vars.darken(&scale, 25.0)?));
        }
        for i in 0..limit {
            let key = format!("cScaleInv{i}");
            let scale = format!("cScale{i}");
            default_to!(vars, &key, json!(vars.adjust(&scale, &[("h", 180.0)])?));
        }

        for i in 0..5 {
            let key = format!("surface{i}");
            default_to!(
                vars,
                &key,
                json!(vars.adjust("mainBkg", &[("h", 30.0), ("l", -(5.0 + i as f64 * 5.0))])?)
            );
            let key = format!("surfacePeer{i}");
            default_to!(
                vars,
                &key,
                json!(vars.adjust("mainBkg", &[("h", 30.0), ("l", -(7.0 + i as f64 * 5.0))])?)
            );
        }
        default_to!(
            vars,
            "scaleLabelColor",
            if vars.truthy("scaleLabelColor") && vars.text("scaleLabelColor") != Some("calculated")
            {
                vars.value("scaleLabelColor")
            } else {
                vars.value("labelTextColor")
            }
        );
        if vars.text("labelTextColor") != Some("calculated") {
            default_to!(vars, "cScaleLabel0", json!(vars.invert("labelTextColor")?));
            default_to!(vars, "cScaleLabel3", json!(vars.invert("labelTextColor")?));
            for i in 0..limit {
                let key = format!("cScaleLabel{i}");
                default_from!(vars, &key, "labelTextColor");
            }
        }

        vars.set("nodeBkg", vars.value("mainBkg"));
        vars.set("nodeBorder", vars.value("border1"));
        vars.set("clusterBkg", vars.value("secondBkg"));
        vars.set("clusterBorder", vars.value("border2"));
        vars.set("defaultLinkColor", vars.value("lineColor"));
        vars.set("titleColor", vars.value("textColor"));
        vars.set("edgeLabelBackground", vars.value("labelBackground"));

        default_from!(vars, "flowContainerStroke", "secondaryBorderColor");

        vars.set("actorBorder", vars.value("border1"));
        vars.set("actorBkg", vars.value("mainBkg"));
        vars.set("labelBoxBkgColor", vars.value("actorBkg"));
        vars.set("signalColor", vars.value("textColor"));
        vars.set("signalTextColor", vars.value("textColor"));
        vars.set("labelBoxBorderColor", vars.value("actorBorder"));
        vars.set("labelTextColor", vars.value("actorTextColor"));
        vars.set("loopTextColor", vars.value("actorTextColor"));
        vars.set("noteBorderColor", vars.value("border2"));
        vars.set("noteTextColor", vars.value("actorTextColor"));
        vars.set("actorLineColor", vars.value("actorBorder"));
        default_from!(vars, "rectBkgColor", "tertiaryColor");

        vars.set("taskTextColor", vars.value("taskTextLightColor"));
        vars.set("taskTextOutsideColor", vars.value("taskTextDarkColor"));

        vars.set("archEdgeColor", vars.value("lineColor"));
        vars.set("archEdgeArrowColor", vars.value("lineColor"));

        default_to!(vars, "rowOdd", json!(vars.lighten("primaryColor", 75.0)?));
        default_to!(vars, "rowEven", json!(vars.lighten("primaryColor", 1.0)?));

        default_from!(vars, "transitionColor", "lineColor");
        default_from!(vars, "transitionLabelColor", "textColor");
        default_to!(
            vars,
            "stateLabelColor",
            if vars.truthy("stateBkg") {
                vars.value("stateBkg")
            } else {
                vars.value("primaryTextColor")
            }
        );
        default_from!(vars, "stateBkg", "mainBkg");
        default_from!(vars, "labelBackgroundColor", "stateBkg");
        default_from!(vars, "compositeBackground", "background");
        default_to!(vars, "altBackground", json!("#f0f0f0"));
        default_from!(vars, "compositeTitleBackground", "mainBkg");
        default_from!(vars, "compositeBorder", "nodeBorder");
        vars.set("innerEndBackground", vars.value("nodeBorder"));
        vars.set("specialStateColor", vars.value("lineColor"));

        default_from!(vars, "errorBkgColor", "tertiaryColor");
        default_from!(vars, "errorTextColor", "tertiaryTextColor");
        default_from!(vars, "transitionColor", "lineColor");

        vars.set("classText", vars.value("primaryTextColor"));

        vars.set("fillType0", vars.value("primaryColor"));
        vars.set("fillType1", vars.value("secondaryColor"));
        vars.set(
            "fillType2",
            json!(vars.adjust("primaryColor", &[("h", 64.0)])?),
        );
        vars.set(
            "fillType3",
            json!(vars.adjust("secondaryColor", &[("h", 64.0)])?),
        );
        vars.set(
            "fillType4",
            json!(vars.adjust("primaryColor", &[("h", -64.0)])?),
        );
        vars.set(
            "fillType5",
            json!(vars.adjust("secondaryColor", &[("h", -64.0)])?),
        );
        vars.set(
            "fillType6",
            json!(vars.adjust("primaryColor", &[("h", 128.0)])?),
        );
        vars.set(
            "fillType7",
            json!(vars.adjust("secondaryColor", &[("h", 128.0)])?),
        );

        default_from!(vars, "pie1", "primaryColor");
        default_from!(vars, "pie2", "secondaryColor");
        default_to!(
            vars,
            "pie3",
            json!(vars.adjust("tertiaryColor", &[("l", -40.0)])?)
        );
        default_to!(
            vars,
            "pie4",
            json!(vars.adjust("primaryColor", &[("l", -10.0)])?)
        );
        default_to!(
            vars,
            "pie5",
            json!(vars.adjust("secondaryColor", &[("l", -30.0)])?)
        );
        default_to!(
            vars,
            "pie6",
            json!(vars.adjust("tertiaryColor", &[("l", -20.0)])?)
        );
        default_to!(
            vars,
            "pie7",
            json!(vars.adjust("primaryColor", &[("h", 60.0), ("l", -20.0)])?)
        );
        default_to!(
            vars,
            "pie8",
            json!(vars.adjust("primaryColor", &[("h", -60.0), ("l", -40.0)])?)
        );
        default_to!(
            vars,
            "pie9",
            json!(vars.adjust("primaryColor", &[("h", 120.0), ("l", -40.0)])?)
        );
        default_to!(
            vars,
            "pie10",
            json!(vars.adjust("primaryColor", &[("h", 60.0), ("l", -40.0)])?)
        );
        default_to!(
            vars,
            "pie11",
            json!(vars.adjust("primaryColor", &[("h", -90.0), ("l", -40.0)])?)
        );
        default_to!(
            vars,
            "pie12",
            json!(vars.adjust("primaryColor", &[("h", 120.0), ("l", -30.0)])?)
        );
        default_to!(vars, "pieTitleTextSize", json!("25px"));
        default_from!(vars, "pieTitleTextColor", "taskTextDarkColor");
        default_to!(vars, "pieSectionTextSize", json!("17px"));
        default_from!(vars, "pieSectionTextColor", "textColor");
        default_to!(vars, "pieLegendTextSize", json!("17px"));
        default_from!(vars, "pieLegendTextColor", "taskTextDarkColor");
        default_to!(vars, "pieStrokeColor", json!("black"));
        default_to!(vars, "pieStrokeWidth", json!("2px"));
        default_to!(vars, "pieOuterStrokeWidth", json!("2px"));
        default_to!(vars, "pieOuterStrokeColor", json!("black"));
        default_to!(vars, "pieOpacity", json!("0.7"));

        default_nullish!(
            vars,
            "venn1",
            json!(vars.adjust("primaryColor", &[("l", -30.0)])?)
        );
        default_nullish!(
            vars,
            "venn2",
            json!(vars.adjust("secondaryColor", &[("l", -30.0)])?)
        );
        default_nullish!(
            vars,
            "venn3",
            json!(vars.adjust("tertiaryColor", &[("l", -40.0)])?)
        );
        default_nullish!(
            vars,
            "venn4",
            json!(vars.adjust("primaryColor", &[("h", 60.0), ("l", -30.0)])?)
        );
        default_nullish!(
            vars,
            "venn5",
            json!(vars.adjust("primaryColor", &[("h", -60.0), ("l", -30.0)])?)
        );
        default_nullish!(
            vars,
            "venn6",
            json!(vars.adjust("secondaryColor", &[("h", 60.0), ("l", -30.0)])?)
        );
        default_nullish!(
            vars,
            "venn7",
            json!(vars.adjust("primaryColor", &[("h", 120.0), ("l", -30.0)])?)
        );
        default_nullish!(
            vars,
            "venn8",
            json!(vars.adjust("secondaryColor", &[("h", 120.0), ("l", -30.0)])?)
        );
        default_nullish!(vars, "vennTitleTextColor", vars.value("titleColor"));
        default_nullish!(vars, "vennSetTextColor", vars.value("textColor"));

        vars.set(
            "cynefin",
            object(vec![
                (
                    "domainFontSize",
                    vars.nested_or_else("cynefin", "domainFontSize", || json!(16)),
                ),
                (
                    "itemFontSize",
                    vars.nested_or_else("cynefin", "itemFontSize", || json!(12)),
                ),
                (
                    "boundaryColor",
                    vars.nested_or("cynefin", "boundaryColor", "lineColor"),
                ),
                (
                    "boundaryWidth",
                    vars.nested_or_else("cynefin", "boundaryWidth", || json!(2)),
                ),
                (
                    "cliffColor",
                    vars.nested_or_else("cynefin", "cliffColor", || json!("#8B0000")),
                ),
                (
                    "cliffWidth",
                    vars.nested_or_else("cynefin", "cliffWidth", || json!(4)),
                ),
                (
                    "arrowColor",
                    vars.nested_or("cynefin", "arrowColor", "lineColor"),
                ),
                (
                    "arrowWidth",
                    vars.nested_or_else("cynefin", "arrowWidth", || json!(2)),
                ),
                (
                    "complexBg",
                    vars.nested_or_else("cynefin", "complexBg", || json!("#E8F5E9")),
                ),
                (
                    "complicatedBg",
                    vars.nested_or_else("cynefin", "complicatedBg", || json!("#E3F2FD")),
                ),
                (
                    "chaoticBg",
                    vars.nested_or_else("cynefin", "chaoticBg", || json!("#FBE9E7")),
                ),
                (
                    "clearBg",
                    vars.nested_or_else("cynefin", "clearBg", || json!("#FFF8E1")),
                ),
                (
                    "confusionBg",
                    vars.nested_or_else("cynefin", "confusionBg", || json!("#F3E5F5")),
                ),
                (
                    "textColor",
                    vars.nested_or("cynefin", "textColor", "textColor"),
                ),
                (
                    "labelColor",
                    vars.nested_or("cynefin", "labelColor", "primaryTextColor"),
                ),
            ]),
        );

        default_from!(vars, "quadrant1Fill", "primaryColor");
        default_to!(
            vars,
            "quadrant2Fill",
            json!(vars.adjust("primaryColor", &[("r", 5.0), ("g", 5.0), ("b", 5.0)])?)
        );
        default_to!(
            vars,
            "quadrant3Fill",
            json!(vars.adjust("primaryColor", &[("r", 10.0), ("g", 10.0), ("b", 10.0)])?)
        );
        default_to!(
            vars,
            "quadrant4Fill",
            json!(vars.adjust("primaryColor", &[("r", 15.0), ("g", 15.0), ("b", 15.0)])?)
        );
        default_from!(vars, "quadrant1TextFill", "primaryTextColor");
        default_to!(
            vars,
            "quadrant2TextFill",
            json!(vars.adjust("primaryTextColor", &[("r", -5.0), ("g", -5.0), ("b", -5.0)])?)
        );
        default_to!(
            vars,
            "quadrant3TextFill",
            json!(vars.adjust(
                "primaryTextColor",
                &[("r", -10.0), ("g", -10.0), ("b", -10.0)]
            )?)
        );
        default_to!(
            vars,
            "quadrant4TextFill",
            json!(vars.adjust(
                "primaryTextColor",
                &[("r", -15.0), ("g", -15.0), ("b", -15.0)]
            )?)
        );
        default_quadrant_point_fill(vars)?;
        default_from!(vars, "quadrantPointTextFill", "primaryTextColor");
        default_from!(vars, "quadrantXAxisTextFill", "primaryTextColor");
        default_from!(vars, "quadrantYAxisTextFill", "primaryTextColor");
        default_from!(
            vars,
            "quadrantInternalBorderStrokeFill",
            "primaryBorderColor"
        );
        default_from!(
            vars,
            "quadrantExternalBorderStrokeFill",
            "primaryBorderColor"
        );
        default_from!(vars, "quadrantTitleFill", "primaryTextColor");

        vars.set(
            "radar",
            object(vec![
                (
                    "axisColor",
                    vars.nested_or("radar", "axisColor", "lineColor"),
                ),
                (
                    "axisStrokeWidth",
                    vars.nested_or_else("radar", "axisStrokeWidth", || json!(2)),
                ),
                (
                    "axisLabelFontSize",
                    vars.nested_or_else("radar", "axisLabelFontSize", || json!(12)),
                ),
                (
                    "curveOpacity",
                    vars.nested_or_else("radar", "curveOpacity", || json!(0.5)),
                ),
                (
                    "curveStrokeWidth",
                    vars.nested_or_else("radar", "curveStrokeWidth", || json!(2)),
                ),
                (
                    "graticuleColor",
                    vars.nested_or_else("radar", "graticuleColor", || json!("#DEDEDE")),
                ),
                (
                    "graticuleStrokeWidth",
                    vars.nested_or_else("radar", "graticuleStrokeWidth", || json!(1)),
                ),
                (
                    "graticuleOpacity",
                    vars.nested_or_else("radar", "graticuleOpacity", || json!(0.3)),
                ),
                (
                    "legendBoxSize",
                    vars.nested_or_else("radar", "legendBoxSize", || json!(12)),
                ),
                (
                    "legendFontSize",
                    vars.nested_or_else("radar", "legendFontSize", || json!(12)),
                ),
            ]),
        );

        default_to!(vars, "wardleyEvolutionColor", json!("#dc3545"));
        vars.set(
            "wardley",
            object(vec![
                (
                    "backgroundColor",
                    vars.nested_or("wardley", "backgroundColor", "background"),
                ),
                (
                    "axisColor",
                    vars.nested_or("wardley", "axisColor", "lineColor"),
                ),
                (
                    "axisTextColor",
                    vars.nested_or("wardley", "axisTextColor", "primaryTextColor"),
                ),
                (
                    "gridColor",
                    vars.nested_or("wardley", "gridColor", "gridColor"),
                ),
                (
                    "componentFill",
                    vars.nested_or("wardley", "componentFill", "background"),
                ),
                (
                    "componentStroke",
                    vars.nested_or("wardley", "componentStroke", "lineColor"),
                ),
                (
                    "componentLabelColor",
                    vars.nested_or("wardley", "componentLabelColor", "primaryTextColor"),
                ),
                (
                    "linkStroke",
                    vars.nested_or("wardley", "linkStroke", "lineColor"),
                ),
                (
                    "evolutionStroke",
                    vars.nested_or("wardley", "evolutionStroke", "wardleyEvolutionColor"),
                ),
                (
                    "annotationStroke",
                    vars.nested_or("wardley", "annotationStroke", "lineColor"),
                ),
                (
                    "annotationTextColor",
                    vars.nested_or("wardley", "annotationTextColor", "primaryTextColor"),
                ),
                (
                    "annotationFill",
                    vars.nested_or("wardley", "annotationFill", "background"),
                ),
            ]),
        );

        vars.set(
            "xyChart",
            object(vec![
                (
                    "backgroundColor",
                    vars.nested_or("xyChart", "backgroundColor", "background"),
                ),
                (
                    "titleColor",
                    vars.nested_or("xyChart", "titleColor", "primaryTextColor"),
                ),
                (
                    "dataLabelColor",
                    vars.nested_or("xyChart", "dataLabelColor", "primaryTextColor"),
                ),
                (
                    "legendTextColor",
                    vars.nested_or("xyChart", "legendTextColor", "primaryTextColor"),
                ),
                (
                    "xAxisTitleColor",
                    vars.nested_or("xyChart", "xAxisTitleColor", "primaryTextColor"),
                ),
                (
                    "xAxisLabelColor",
                    vars.nested_or("xyChart", "xAxisLabelColor", "primaryTextColor"),
                ),
                (
                    "xAxisTickColor",
                    vars.nested_or("xyChart", "xAxisTickColor", "primaryTextColor"),
                ),
                (
                    "xAxisLineColor",
                    vars.nested_or("xyChart", "xAxisLineColor", "primaryTextColor"),
                ),
                (
                    "yAxisTitleColor",
                    vars.nested_or("xyChart", "yAxisTitleColor", "primaryTextColor"),
                ),
                (
                    "yAxisLabelColor",
                    vars.nested_or("xyChart", "yAxisLabelColor", "primaryTextColor"),
                ),
                (
                    "yAxisTickColor",
                    vars.nested_or("xyChart", "yAxisTickColor", "primaryTextColor"),
                ),
                (
                    "yAxisLineColor",
                    vars.nested_or("xyChart", "yAxisLineColor", "primaryTextColor"),
                ),
                (
                    "plotColorPalette",
                    vars.nested_or_else("xyChart", "plotColorPalette", || {
                        json!(
                            "#ECECFF,#8493A6,#FFC3A0,#DCDDE1,#B8E994,#D1A36F,#C3CDE6,#FFB6C1,#496078,#F8F3E3"
                        )
                    }),
                ),
            ]),
        );

        default_from!(vars, "requirementBackground", "primaryColor");
        default_from!(vars, "requirementBorderColor", "primaryBorderColor");
        default_to!(vars, "requirementBorderSize", json!("1"));
        default_from!(vars, "requirementTextColor", "primaryTextColor");
        default_from!(vars, "relationColor", "lineColor");
        default_from!(vars, "relationLabelBackground", "labelBackground");
        default_from!(vars, "relationLabelColor", "actorTextColor");

        default_from!(vars, "git0", "primaryColor");
        default_from!(vars, "git1", "secondaryColor");
        default_from!(vars, "git2", "tertiaryColor");
        default_to!(
            vars,
            "git3",
            json!(vars.adjust("primaryColor", &[("h", -30.0)])?)
        );
        default_to!(
            vars,
            "git4",
            json!(vars.adjust("primaryColor", &[("h", -60.0)])?)
        );
        default_to!(
            vars,
            "git5",
            json!(vars.adjust("primaryColor", &[("h", -90.0)])?)
        );
        default_to!(
            vars,
            "git6",
            json!(vars.adjust("primaryColor", &[("h", 60.0)])?)
        );
        default_to!(
            vars,
            "git7",
            json!(vars.adjust("primaryColor", &[("h", 120.0)])?)
        );
        for i in 0..8 {
            let key = format!("git{i}");
            let value = if vars.dark_mode() {
                json!(vars.lighten(&key, 25.0)?)
            } else {
                json!(vars.darken(&key, 25.0)?)
            };
            vars.set(&key, value);
        }
        default_to!(
            vars,
            "gitInv0",
            json!(vars.darken_color(&vars.invert("git0")?, 25.0)?)
        );
        for i in 1..8 {
            let key = format!("gitInv{i}");
            let source = format!("git{i}");
            default_to!(vars, &key, json!(vars.invert(&source)?));
        }
        default_to!(
            vars,
            "gitBranchLabel0",
            json!(vars.invert("labelTextColor")?)
        );
        default_from!(vars, "gitBranchLabel1", "labelTextColor");
        default_from!(vars, "gitBranchLabel2", "labelTextColor");
        default_to!(
            vars,
            "gitBranchLabel3",
            json!(vars.invert("labelTextColor")?)
        );
        default_from!(vars, "gitBranchLabel4", "labelTextColor");
        default_from!(vars, "gitBranchLabel5", "labelTextColor");
        default_from!(vars, "gitBranchLabel6", "labelTextColor");
        default_from!(vars, "gitBranchLabel7", "labelTextColor");

        default_from!(vars, "tagLabelColor", "primaryTextColor");
        default_from!(vars, "tagLabelBackground", "primaryColor");
        default_to!(
            vars,
            "tagLabelBorder",
            if vars.truthy("tagBorder") {
                vars.value("tagBorder")
            } else {
                vars.value("primaryBorderColor")
            }
        );
        default_to!(vars, "tagLabelFontSize", json!("10px"));
        default_from!(vars, "commitLabelColor", "secondaryTextColor");
        default_from!(vars, "commitLabelBackground", "secondaryColor");
        default_to!(vars, "commitLabelFontSize", json!("10px"));

        default_to!(vars, "emUiFill", json!("white"));
        default_to!(vars, "emUiStroke", json!("#dbdada"));
        default_to!(vars, "emProcessorFill", json!("#edb3f6"));
        default_to!(vars, "emProcessorStroke", json!("#b88cbf"));
        default_to!(vars, "emReadModelFill", json!("#d3f1a2"));
        default_to!(vars, "emReadModelStroke", json!("#a3b732"));
        default_to!(vars, "emCommandFill", json!("#bcd6fe"));
        default_to!(vars, "emCommandStroke", json!("#679ac3"));
        default_to!(vars, "emEventFill", json!("#ffb778"));
        default_to!(vars, "emEventStroke", json!("#c19a0f"));
        default_to!(vars, "emSwimlaneBackgroundOdd", json!("rgb(250,250,250)"));
        default_to!(
            vars,
            "emSwimlaneBackgroundStroke",
            json!("rgb(240,240,240)")
        );
        default_from!(vars, "emArrowhead", "lineColor");
        default_from!(vars, "emRelationStroke", "lineColor");

        default_to!(
            vars,
            "attributeBackgroundColorOdd",
            json!(OLD_ATTRIBUTE_ODD)
        );
        default_to!(
            vars,
            "attributeBackgroundColorEven",
            json!(OLD_ATTRIBUTE_EVEN)
        );
        Ok(())
    }

    fn color_limit(vars: &Vars) -> usize {
        vars.values
            .get("THEME_COLOR_LIMIT")
            .and_then(Value::as_f64)
            .unwrap_or(12.0) as usize
    }

    fn mk_border(value: &str, dark_mode: bool) -> Result<String, ThemeError> {
        let lightness = if dark_mode { 10.0 } else { -10.0 };
        Ok(color::adjust(value, &[("s", -40.0), ("l", lightness)])?)
    }
}
mod dark_theme {
    use super::*;

    pub(super) fn constructor() -> Vars {
        let mut vars = Vars::new();
        vars.set_text("background", "#333");
        vars.set_text("primaryColor", "#1f2020");
        vars.set_text(
            "secondaryColor",
            &color::lighten("#1f2020", 16.0).expect("a hex color"),
        );
        vars.set_text(
            "tertiaryColor",
            &color::adjust("#1f2020", &[("h", -160.0)]).expect("a hex color"),
        );
        vars.set_text(
            "primaryBorderColor",
            &color::invert("#333", 100.0).expect("a hex color"),
        );
        let secondary = color::lighten("#1f2020", 16.0).expect("a hex color");
        let tertiary = color::adjust("#1f2020", &[("h", -160.0)]).expect("a hex color");
        vars.set_text(
            "secondaryBorderColor",
            &color::adjust(&secondary, &[("s", -40.0), ("l", -10.0)]).expect("a valid color"),
        );
        vars.set_text(
            "tertiaryBorderColor",
            &color::adjust(&tertiary, &[("s", -40.0), ("l", -10.0)]).expect("a valid color"),
        );
        vars.set_text(
            "primaryTextColor",
            &color::invert("#1f2020", 100.0).expect("a hex color"),
        );
        vars.set_text(
            "secondaryTextColor",
            &color::invert(&secondary, 100.0).expect("a valid color"),
        );
        vars.set_text(
            "tertiaryTextColor",
            &color::invert(&tertiary, 100.0).expect("a valid color"),
        );
        vars.set_text(
            "lineColor",
            &color::invert("#333", 100.0).expect("a hex color"),
        );
        vars.set_text(
            "textColor",
            &color::invert("#333", 100.0).expect("a hex color"),
        );

        vars.set_text("mainBkg", "#1f2020");
        vars.set_text("mainContrastColor", "lightgrey");
        vars.set_text(
            "darkTextColor",
            &color::lighten(&color::invert("#323D47", 100.0).expect("a hex color"), 10.0)
                .expect("a valid color"),
        );
        vars.set_text("border1", "#ccc");
        vars.set_text("border2", &color::rgba(255.0, 255.0, 255.0, 0.25));
        vars.set_text("fontFamily", "\"trebuchet ms\", verdana, arial, sans-serif");
        vars.set_text("fontSize", "16px");
        vars.set_text("labelBackground", "#181818");
        vars.set_text("textColor", "#ccc");
        vars.set("THEME_COLOR_LIMIT", json!(12));
        vars.set("radius", json!(5));
        vars.set("strokeWidth", json!(1));

        vars.set_text("titleColor", "#F9FFFE");
        vars.set_text("noteBkgColor", "#fff5ad");
        vars.set_text("sequenceNumberColor", "black");
        vars.set_text("clusterBkg", "#302F3D");

        vars.set_text(
            "sectionBkgColor",
            &color::darken("#EAE8D9", 30.0).expect("a hex color"),
        );
        vars.set_text("sectionBkgColor2", "#EAE8D9");
        vars.set_text(
            "excludeBkgColor",
            &color::darken(&color::darken("#EAE8D9", 30.0).expect("a hex color"), 10.0)
                .expect("a valid color"),
        );
        vars.set_text("taskBorderColor", &color::rgba(255.0, 255.0, 255.0, 70.0));
        vars.set_text(
            "activeTaskBorderColor",
            &color::rgba(255.0, 255.0, 255.0, 50.0),
        );
        vars.set_text("activeTaskBkgColor", "#81B1DB");
        vars.set_text("doneTaskBorderColor", "grey");
        vars.set_text("critBorderColor", "#E83737");
        vars.set_text("critBkgColor", "#E83737");
        vars.set_text("todayLineColor", "#DB5757");
        vars.set_text("vertLineColor", "#00BFFF");
        vars.set_text("taskTextClickableColor", "#003163");

        let primary_border = color::invert("#333", 100.0).expect("a hex color");
        vars.set_text("personBorder", &primary_border);
        vars.set_text("personBkg", "#1f2020");

        vars.set_text("archEdgeWidth", "3");
        vars.set_text("archGroupBorderColor", &primary_border);
        vars.set_text("archGroupBorderWidth", "2px");

        vars.set_text(
            "rowOdd",
            &color::lighten("#1f2020", 5.0).expect("a hex color"),
        );
        vars.set_text(
            "rowEven",
            &color::darken("#1f2020", 10.0).expect("a hex color"),
        );

        vars.set_text("labelColor", "calculated");
        vars.set_text("errorBkgColor", "#a44141");
        vars.set_text("errorTextColor", "#ddd");
        vars.set("useGradient", json!(true));
        vars.set_text("gradientStart", &primary_border);
        vars.set_text(
            "gradientStop",
            &color::adjust(
                &color::lighten("#1f2020", 16.0).expect("a hex color"),
                &[("s", -40.0), ("l", -10.0)],
            )
            .expect("a valid color"),
        );
        vars.set_text(
            "dropShadow",
            "drop-shadow( 1px 2px 2px rgba(185,185,185,1))",
        );

        vars.set_text("noteFontWeight", "normal");
        vars.set_text("fontWeight", "normal");
        vars
    }

    pub(super) fn update_colors(vars: &mut Vars) -> Result<(), ThemeError> {
        vars.set("secondBkg", json!(vars.lighten("mainBkg", 16.0)?));
        vars.set("lineColor", vars.value("mainContrastColor"));
        vars.set("arrowheadColor", vars.value("mainContrastColor"));

        vars.set("nodeBkg", vars.value("mainBkg"));
        vars.set("nodeBorder", vars.value("border1"));
        vars.set("clusterBkg", vars.value("secondBkg"));
        vars.set("clusterBorder", vars.value("border2"));
        vars.set("defaultLinkColor", vars.value("lineColor"));
        vars.set(
            "edgeLabelBackground",
            json!(vars.lighten("labelBackground", 25.0)?),
        );

        default_from!(vars, "flowContainerStroke", "secondaryBorderColor");

        vars.set("actorBorder", vars.value("border1"));
        vars.set("actorBkg", vars.value("mainBkg"));
        vars.set("actorTextColor", vars.value("mainContrastColor"));
        vars.set("actorLineColor", vars.value("actorBorder"));
        vars.set("signalColor", vars.value("mainContrastColor"));
        vars.set("signalTextColor", vars.value("mainContrastColor"));
        vars.set("labelBoxBkgColor", vars.value("actorBkg"));
        vars.set("labelBoxBorderColor", vars.value("actorBorder"));
        vars.set("labelTextColor", vars.value("mainContrastColor"));
        vars.set("loopTextColor", vars.value("mainContrastColor"));
        vars.set("noteBorderColor", vars.value("secondaryBorderColor"));
        vars.set("noteBkgColor", vars.value("secondBkg"));
        vars.set("noteTextColor", vars.value("secondaryTextColor"));
        vars.set("activationBorderColor", vars.value("border1"));
        vars.set("activationBkgColor", vars.value("secondBkg"));
        default_from!(vars, "rectBkgColor", "tertiaryColor");

        vars.set("altSectionBkgColor", vars.value("background"));
        vars.set("taskBkgColor", json!(vars.lighten("mainBkg", 23.0)?));
        vars.set("taskTextColor", vars.value("darkTextColor"));
        vars.set("taskTextLightColor", vars.value("mainContrastColor"));
        vars.set("taskTextOutsideColor", vars.value("taskTextLightColor"));
        vars.set("gridColor", vars.value("mainContrastColor"));
        vars.set("doneTaskBkgColor", vars.value("mainContrastColor"));
        vars.set("taskTextDarkColor", json!(vars.invert("doneTaskBkgColor")?));

        vars.set("archEdgeColor", vars.value("lineColor"));
        vars.set("archEdgeArrowColor", vars.value("lineColor"));

        default_from!(vars, "transitionColor", "lineColor");
        default_from!(vars, "transitionLabelColor", "textColor");
        default_to!(
            vars,
            "stateLabelColor",
            if vars.truthy("stateBkg") {
                vars.value("stateBkg")
            } else {
                vars.value("primaryTextColor")
            }
        );
        default_from!(vars, "stateBkg", "mainBkg");
        default_from!(vars, "labelBackgroundColor", "stateBkg");
        default_from!(vars, "compositeBackground", "background");
        default_to!(vars, "altBackground", json!("#555"));
        default_from!(vars, "compositeTitleBackground", "mainBkg");
        default_from!(vars, "compositeBorder", "nodeBorder");
        vars.set("innerEndBackground", vars.value("primaryBorderColor"));
        vars.set_text("specialStateColor", "#f4f4f4");

        default_from!(vars, "errorBkgColor", "tertiaryColor");
        default_from!(vars, "errorTextColor", "tertiaryTextColor");

        vars.set("fillType0", vars.value("primaryColor"));
        vars.set("fillType1", vars.value("secondaryColor"));
        vars.set(
            "fillType2",
            json!(vars.adjust("primaryColor", &[("h", 64.0)])?),
        );
        vars.set(
            "fillType3",
            json!(vars.adjust("secondaryColor", &[("h", 64.0)])?),
        );
        vars.set(
            "fillType4",
            json!(vars.adjust("primaryColor", &[("h", -64.0)])?),
        );
        vars.set(
            "fillType5",
            json!(vars.adjust("secondaryColor", &[("h", -64.0)])?),
        );
        vars.set(
            "fillType6",
            json!(vars.adjust("primaryColor", &[("h", 128.0)])?),
        );
        vars.set(
            "fillType7",
            json!(vars.adjust("secondaryColor", &[("h", 128.0)])?),
        );

        for (key, value) in [
            ("cScale1", "#0b0000"),
            ("cScale2", "#4d1037"),
            ("cScale3", "#3f5258"),
            ("cScale4", "#4f2f1b"),
            ("cScale5", "#6e0a0a"),
            ("cScale6", "#3b0048"),
            ("cScale7", "#995a01"),
            ("cScale8", "#154706"),
            ("cScale9", "#161722"),
            ("cScale10", "#00296f"),
            ("cScale11", "#01629c"),
            ("cScale12", "#010029"),
        ] {
            default_to!(vars, key, json!(value));
        }

        let limit = color_limit(vars);
        for i in 0..limit {
            let key = format!("cScale{i}");
            default_to!(
                vars,
                &key,
                match i {
                    0 => vars.value("primaryColor"),
                    1 => vars.value("secondaryColor"),
                    2 => vars.value("tertiaryColor"),
                    3 => json!(vars.adjust("primaryColor", &[("h", 30.0)])?),
                    4 => json!(vars.adjust("primaryColor", &[("h", 60.0)])?),
                    5 => json!(vars.adjust("primaryColor", &[("h", 90.0)])?),
                    6 => json!(vars.adjust("primaryColor", &[("h", 120.0)])?),
                    7 => json!(vars.adjust("primaryColor", &[("h", 150.0)])?),
                    8 => json!(vars.adjust("primaryColor", &[("h", 210.0)])?),
                    9 => json!(vars.adjust("primaryColor", &[("h", 270.0)])?),
                    10 => json!(vars.adjust("primaryColor", &[("h", 300.0)])?),
                    _ => json!(vars.adjust("primaryColor", &[("h", 330.0)])?),
                }
            );
        }
        for i in 0..limit {
            let key = format!("cScaleInv{i}");
            let scale = format!("cScale{i}");
            default_to!(vars, &key, json!(vars.invert(&scale)?));
        }
        for i in 0..limit {
            let key = format!("cScalePeer{i}");
            let scale = format!("cScale{i}");
            default_to!(vars, &key, json!(vars.lighten(&scale, 10.0)?));
        }

        for i in 0..5 {
            let key = format!("surface{i}");
            default_to!(
                vars,
                &key,
                json!(vars.adjust(
                    "mainBkg",
                    &[("h", 30.0), ("s", -30.0), ("l", -(-10.0 + i as f64 * 4.0))]
                )?)
            );
            let key = format!("surfacePeer{i}");
            default_to!(
                vars,
                &key,
                json!(vars.adjust(
                    "mainBkg",
                    &[("h", 30.0), ("s", -30.0), ("l", -(-7.0 + i as f64 * 4.0))]
                )?)
            );
        }

        default_to!(
            vars,
            "scaleLabelColor",
            if vars.dark_mode() {
                json!("black")
            } else {
                vars.value("labelTextColor")
            }
        );
        for i in 0..limit {
            let key = format!("cScaleLabel{i}");
            default_from!(vars, &key, "scaleLabelColor");
        }

        for i in 0..limit {
            let pie = format!("pie{i}");
            let scale = format!("cScale{i}");
            vars.set(&pie, vars.value(&scale));
        }
        default_to!(vars, "pieTitleTextSize", json!("25px"));
        default_from!(vars, "pieTitleTextColor", "mainContrastColor");
        default_to!(vars, "pieSectionTextSize", json!("17px"));
        default_from!(vars, "pieSectionTextColor", "textColor");
        default_to!(vars, "pieLegendTextSize", json!("17px"));
        default_from!(vars, "pieLegendTextColor", "mainContrastColor");
        default_to!(vars, "pieStrokeColor", json!("black"));
        default_to!(vars, "pieStrokeWidth", json!("2px"));
        default_to!(vars, "pieOuterStrokeWidth", json!("2px"));
        default_to!(vars, "pieOuterStrokeColor", json!("black"));
        default_to!(vars, "pieOpacity", json!("0.7"));

        for i in 0..8 {
            let venn = format!("venn{}", i + 1);
            let scale = format!("cScale{i}");
            default_nullish!(vars, &venn, json!(vars.lighten(&scale, 30.0)?));
        }
        default_nullish!(vars, "vennTitleTextColor", vars.value("titleColor"));
        default_nullish!(vars, "vennSetTextColor", vars.value("textColor"));

        vars.set(
            "cynefin",
            object(vec![
                (
                    "domainFontSize",
                    vars.nested_or_else("cynefin", "domainFontSize", || json!(16)),
                ),
                (
                    "itemFontSize",
                    vars.nested_or_else("cynefin", "itemFontSize", || json!(12)),
                ),
                (
                    "boundaryColor",
                    vars.nested_or("cynefin", "boundaryColor", "lineColor"),
                ),
                (
                    "boundaryWidth",
                    vars.nested_or_else("cynefin", "boundaryWidth", || json!(2)),
                ),
                (
                    "cliffColor",
                    vars.nested_or_else("cynefin", "cliffColor", || json!("#FF6B6B")),
                ),
                (
                    "cliffWidth",
                    vars.nested_or_else("cynefin", "cliffWidth", || json!(4)),
                ),
                (
                    "arrowColor",
                    vars.nested_or("cynefin", "arrowColor", "lineColor"),
                ),
                (
                    "arrowWidth",
                    vars.nested_or_else("cynefin", "arrowWidth", || json!(2)),
                ),
                (
                    "complexBg",
                    vars.nested_or_else("cynefin", "complexBg", || json!("#1B5E20")),
                ),
                (
                    "complicatedBg",
                    vars.nested_or_else("cynefin", "complicatedBg", || json!("#0D47A1")),
                ),
                (
                    "chaoticBg",
                    vars.nested_or_else("cynefin", "chaoticBg", || json!("#BF360C")),
                ),
                (
                    "clearBg",
                    vars.nested_or_else("cynefin", "clearBg", || json!("#F57F17")),
                ),
                (
                    "confusionBg",
                    vars.nested_or_else("cynefin", "confusionBg", || json!("#4A148C")),
                ),
                (
                    "textColor",
                    vars.nested_or("cynefin", "textColor", "textColor"),
                ),
                (
                    "labelColor",
                    vars.nested_or("cynefin", "labelColor", "primaryTextColor"),
                ),
            ]),
        );

        default_from!(vars, "quadrant1Fill", "primaryColor");
        default_to!(
            vars,
            "quadrant2Fill",
            json!(vars.adjust("primaryColor", &[("r", 5.0), ("g", 5.0), ("b", 5.0)])?)
        );
        default_to!(
            vars,
            "quadrant3Fill",
            json!(vars.adjust("primaryColor", &[("r", 10.0), ("g", 10.0), ("b", 10.0)])?)
        );
        default_to!(
            vars,
            "quadrant4Fill",
            json!(vars.adjust("primaryColor", &[("r", 15.0), ("g", 15.0), ("b", 15.0)])?)
        );
        default_from!(vars, "quadrant1TextFill", "primaryTextColor");
        default_to!(
            vars,
            "quadrant2TextFill",
            json!(vars.adjust("primaryTextColor", &[("r", -5.0), ("g", -5.0), ("b", -5.0)])?)
        );
        default_to!(
            vars,
            "quadrant3TextFill",
            json!(vars.adjust(
                "primaryTextColor",
                &[("r", -10.0), ("g", -10.0), ("b", -10.0)]
            )?)
        );
        default_to!(
            vars,
            "quadrant4TextFill",
            json!(vars.adjust(
                "primaryTextColor",
                &[("r", -15.0), ("g", -15.0), ("b", -15.0)]
            )?)
        );
        default_quadrant_point_fill(vars)?;
        default_from!(vars, "quadrantPointTextFill", "primaryTextColor");
        default_from!(vars, "quadrantXAxisTextFill", "primaryTextColor");
        default_from!(vars, "quadrantYAxisTextFill", "primaryTextColor");
        default_from!(
            vars,
            "quadrantInternalBorderStrokeFill",
            "primaryBorderColor"
        );
        default_from!(
            vars,
            "quadrantExternalBorderStrokeFill",
            "primaryBorderColor"
        );
        default_from!(vars, "quadrantTitleFill", "primaryTextColor");

        vars.set(
            "xyChart",
            object(vec![
                (
                    "backgroundColor",
                    vars.nested_or("xyChart", "backgroundColor", "background"),
                ),
                (
                    "titleColor",
                    vars.nested_or("xyChart", "titleColor", "primaryTextColor"),
                ),
                (
                    "dataLabelColor",
                    vars.nested_or("xyChart", "dataLabelColor", "primaryTextColor"),
                ),
                (
                    "legendTextColor",
                    vars.nested_or("xyChart", "legendTextColor", "primaryTextColor"),
                ),
                (
                    "xAxisTitleColor",
                    vars.nested_or("xyChart", "xAxisTitleColor", "primaryTextColor"),
                ),
                (
                    "xAxisLabelColor",
                    vars.nested_or("xyChart", "xAxisLabelColor", "primaryTextColor"),
                ),
                (
                    "xAxisTickColor",
                    vars.nested_or("xyChart", "xAxisTickColor", "primaryTextColor"),
                ),
                (
                    "xAxisLineColor",
                    vars.nested_or("xyChart", "xAxisLineColor", "primaryTextColor"),
                ),
                (
                    "yAxisTitleColor",
                    vars.nested_or("xyChart", "yAxisTitleColor", "primaryTextColor"),
                ),
                (
                    "yAxisLabelColor",
                    vars.nested_or("xyChart", "yAxisLabelColor", "primaryTextColor"),
                ),
                (
                    "yAxisTickColor",
                    vars.nested_or("xyChart", "yAxisTickColor", "primaryTextColor"),
                ),
                (
                    "yAxisLineColor",
                    vars.nested_or("xyChart", "yAxisLineColor", "primaryTextColor"),
                ),
                (
                    "plotColorPalette",
                    vars.nested_or_else("xyChart", "plotColorPalette", || {
                        json!(
                            "#3498db,#2ecc71,#e74c3c,#f1c40f,#bdc3c7,#ffffff,#34495e,#9b59b6,#1abc9c,#e67e22"
                        )
                    }),
                ),
            ]),
        );

        vars.set(
            "packet",
            object(vec![
                ("startByteColor", vars.value("primaryTextColor")),
                ("endByteColor", vars.value("primaryTextColor")),
                ("labelColor", vars.value("primaryTextColor")),
                ("titleColor", vars.value("primaryTextColor")),
                ("blockStrokeColor", vars.value("primaryTextColor")),
                ("blockFillColor", vars.value("background")),
            ]),
        );

        vars.set(
            "radar",
            object(vec![
                (
                    "axisColor",
                    vars.nested_or("radar", "axisColor", "lineColor"),
                ),
                (
                    "axisStrokeWidth",
                    vars.nested_or_else("radar", "axisStrokeWidth", || json!(2)),
                ),
                (
                    "axisLabelFontSize",
                    vars.nested_or_else("radar", "axisLabelFontSize", || json!(12)),
                ),
                (
                    "curveOpacity",
                    vars.nested_or_else("radar", "curveOpacity", || json!(0.5)),
                ),
                (
                    "curveStrokeWidth",
                    vars.nested_or_else("radar", "curveStrokeWidth", || json!(2)),
                ),
                (
                    "graticuleColor",
                    vars.nested_or_else("radar", "graticuleColor", || json!("#DEDEDE")),
                ),
                (
                    "graticuleStrokeWidth",
                    vars.nested_or_else("radar", "graticuleStrokeWidth", || json!(1)),
                ),
                (
                    "graticuleOpacity",
                    vars.nested_or_else("radar", "graticuleOpacity", || json!(0.3)),
                ),
                (
                    "legendBoxSize",
                    vars.nested_or_else("radar", "legendBoxSize", || json!(12)),
                ),
                (
                    "legendFontSize",
                    vars.nested_or_else("radar", "legendFontSize", || json!(12)),
                ),
            ]),
        );

        default_to!(vars, "wardleyEvolutionColor", json!("#ff6b6b"));
        vars.set(
            "wardley",
            object(vec![
                (
                    "backgroundColor",
                    vars.nested_or("wardley", "backgroundColor", "background"),
                ),
                (
                    "axisColor",
                    vars.nested_or("wardley", "axisColor", "lineColor"),
                ),
                (
                    "axisTextColor",
                    vars.nested_or("wardley", "axisTextColor", "primaryTextColor"),
                ),
                (
                    "gridColor",
                    vars.nested_or("wardley", "gridColor", "gridColor"),
                ),
                (
                    "componentFill",
                    vars.nested_or("wardley", "componentFill", "mainBkg"),
                ),
                (
                    "componentStroke",
                    vars.nested_or("wardley", "componentStroke", "lineColor"),
                ),
                (
                    "componentLabelColor",
                    vars.nested_or("wardley", "componentLabelColor", "primaryTextColor"),
                ),
                (
                    "linkStroke",
                    vars.nested_or("wardley", "linkStroke", "lineColor"),
                ),
                (
                    "evolutionStroke",
                    vars.nested_or("wardley", "evolutionStroke", "wardleyEvolutionColor"),
                ),
                (
                    "annotationStroke",
                    vars.nested_or("wardley", "annotationStroke", "lineColor"),
                ),
                (
                    "annotationTextColor",
                    vars.nested_or("wardley", "annotationTextColor", "primaryTextColor"),
                ),
                (
                    "annotationFill",
                    vars.nested_or("wardley", "annotationFill", "mainBkg"),
                ),
            ]),
        );

        vars.set("classText", vars.value("primaryTextColor"));

        default_from!(vars, "requirementBackground", "primaryColor");
        default_from!(vars, "requirementBorderColor", "primaryBorderColor");
        default_to!(vars, "requirementBorderSize", json!("1"));
        default_from!(vars, "requirementTextColor", "primaryTextColor");
        default_from!(vars, "relationColor", "lineColor");
        default_to!(
            vars,
            "relationLabelBackground",
            if vars.dark_mode() {
                json!(vars.darken("secondaryColor", 30.0)?)
            } else {
                vars.value("secondaryColor")
            }
        );
        default_from!(vars, "relationLabelColor", "actorTextColor");

        vars.set("git0", json!(vars.lighten("secondaryColor", 20.0)?));
        vars.set(
            "git1",
            json!(
                vars.lighten("pie2", 20.0)
                    .or_else(|_| vars.lighten("secondaryColor", 20.0))?
            ),
        );
        vars.set(
            "git2",
            json!(
                vars.lighten("pie3", 20.0)
                    .or_else(|_| vars.lighten("tertiaryColor", 20.0))?
            ),
        );
        vars.set(
            "git3",
            json!(
                vars.lighten("pie4", 20.0)
                    .or_else(|_| { vars.adjust("primaryColor", &[("h", -30.0)]) })?
            ),
        );
        vars.set(
            "git4",
            json!(
                vars.lighten("pie5", 20.0)
                    .or_else(|_| { vars.adjust("primaryColor", &[("h", -60.0)]) })?
            ),
        );
        vars.set(
            "git5",
            json!(
                vars.lighten("pie6", 10.0)
                    .or_else(|_| { vars.adjust("primaryColor", &[("h", -90.0)]) })?
            ),
        );
        vars.set(
            "git6",
            json!(
                vars.lighten("pie7", 10.0)
                    .or_else(|_| { vars.adjust("primaryColor", &[("h", 60.0)]) })?
            ),
        );
        vars.set(
            "git7",
            json!(
                vars.lighten("pie8", 20.0)
                    .or_else(|_| { vars.adjust("primaryColor", &[("h", 120.0)]) })?
            ),
        );
        for i in 0..8 {
            let key = format!("gitInv{i}");
            let source = format!("git{i}");
            default_to!(vars, &key, json!(vars.invert(&source)?));
        }
        default_to!(
            vars,
            "gitBranchLabel0",
            json!(vars.invert("labelTextColor")?)
        );
        default_from!(vars, "gitBranchLabel1", "labelTextColor");
        default_from!(vars, "gitBranchLabel2", "labelTextColor");
        default_to!(
            vars,
            "gitBranchLabel3",
            json!(vars.invert("labelTextColor")?)
        );
        default_from!(vars, "gitBranchLabel4", "labelTextColor");
        default_from!(vars, "gitBranchLabel5", "labelTextColor");
        default_from!(vars, "gitBranchLabel6", "labelTextColor");
        default_from!(vars, "gitBranchLabel7", "labelTextColor");

        default_from!(vars, "tagLabelColor", "primaryTextColor");
        default_from!(vars, "tagLabelBackground", "primaryColor");
        default_to!(
            vars,
            "tagLabelBorder",
            if vars.truthy("tagBorder") {
                vars.value("tagBorder")
            } else {
                vars.value("primaryBorderColor")
            }
        );
        default_to!(vars, "tagLabelFontSize", json!("10px"));
        default_from!(vars, "commitLabelColor", "secondaryTextColor");
        default_from!(vars, "commitLabelBackground", "secondaryColor");
        default_to!(vars, "commitLabelFontSize", json!("10px"));

        default_to!(vars, "emUiFill", json!("#2d2d2d"));
        default_to!(vars, "emUiStroke", json!("#555"));
        default_to!(
            vars,
            "emProcessorFill",
            json!(vars.lighten_color("#5a3d5c", 10.0)?)
        );
        default_to!(vars, "emProcessorStroke", json!("#8a6d8c"));
        default_to!(
            vars,
            "emReadModelFill",
            json!(vars.lighten_color("#3d5a2d", 10.0)?)
        );
        default_to!(vars, "emReadModelStroke", json!("#6d8c5c"));
        default_to!(
            vars,
            "emCommandFill",
            json!(vars.lighten_color("#2d3d5a", 10.0)?)
        );
        default_to!(vars, "emCommandStroke", json!("#5c6d8c"));
        default_to!(
            vars,
            "emEventFill",
            json!(vars.lighten_color("#5a452d", 10.0)?)
        );
        default_to!(vars, "emEventStroke", json!("#8c755c"));
        default_to!(
            vars,
            "emSwimlaneBackgroundOdd",
            json!(vars.lighten("background", 5.0)?)
        );
        default_to!(
            vars,
            "emSwimlaneBackgroundStroke",
            json!(vars.lighten("background", 12.0)?)
        );
        default_from!(vars, "emArrowhead", "lineColor");
        default_from!(vars, "emRelationStroke", "lineColor");

        default_to!(
            vars,
            "attributeBackgroundColorOdd",
            json!(vars.lighten("background", 12.0)?)
        );
        default_to!(
            vars,
            "attributeBackgroundColorEven",
            json!(vars.lighten("background", 2.0)?)
        );

        default_to!(vars, "nodeBorder", json!("#999"));
        Ok(())
    }

    fn color_limit(vars: &Vars) -> usize {
        vars.values
            .get("THEME_COLOR_LIMIT")
            .and_then(Value::as_f64)
            .unwrap_or(12.0) as usize
    }
}
mod forest_theme {
    use super::*;

    const OLD_ATTRIBUTE_ODD: &str = "#ffffff";
    const OLD_ATTRIBUTE_EVEN: &str = "#f2f2f2";

    pub(super) fn constructor() -> Result<Vars, ThemeError> {
        let mut vars = Vars::new();
        vars.set_text("background", "#f4f4f4");
        vars.set_text("primaryColor", "#cde498");
        vars.set_text("secondaryColor", "#cdffb2");
        vars.set_text("background", "white");
        vars.set_text("mainBkg", "#cde498");
        vars.set_text("secondBkg", "#cdffb2");
        vars.set_text("lineColor", "green");
        vars.set_text("border1", "#13540c");
        vars.set_text("border2", "#6eaa49");
        vars.set_text("arrowheadColor", "green");
        vars.set_text("fontFamily", "\"trebuchet ms\", verdana, arial, sans-serif");
        vars.set_text("fontSize", "16px");

        let tertiary = color::lighten("#cde498", 10.0)?;
        vars.set_text("tertiaryColor", &tertiary);
        let primary_border = color::adjust("#cde498", &[("s", -40.0), ("l", -10.0)])?;
        let secondary_border = color::adjust("#cdffb2", &[("s", -40.0), ("l", -10.0)])?;
        let tertiary_border = color::adjust(&tertiary, &[("s", -40.0), ("l", -10.0)])?;
        vars.set_text("primaryBorderColor", &primary_border);
        vars.set_text("secondaryBorderColor", &secondary_border);
        vars.set_text("tertiaryBorderColor", &tertiary_border);
        vars.set_text("primaryTextColor", &color::invert("#cde498", 100.0)?);
        vars.set_text("secondaryTextColor", &color::invert("#cdffb2", 100.0)?);
        vars.set_text("tertiaryTextColor", &color::invert("#cde498", 100.0)?);
        vars.set_text("lineColor", &color::invert("white", 100.0)?);
        vars.set_text("textColor", &color::invert("white", 100.0)?);
        vars.set("THEME_COLOR_LIMIT", json!(12));
        vars.set("radius", json!(5));
        vars.set("strokeWidth", json!(1));

        vars.set_text("titleColor", "#333");
        vars.set_text("edgeLabelBackground", "#e8e8e8");

        vars.set_text("actorTextColor", "black");
        vars.set_text("signalColor", "#333");
        vars.set_text("signalTextColor", "#333");
        vars.set_text("labelBoxBorderColor", "#326932");
        vars.set_text("noteBkgColor", "#fff5ad");
        vars.set_text("activationBorderColor", "#666");
        vars.set_text("activationBkgColor", "#f4f4f4");
        vars.set_text("sequenceNumberColor", "white");

        vars.set_text("sectionBkgColor", "#6eaa49");
        vars.set_text("altSectionBkgColor", "white");
        vars.set_text("sectionBkgColor2", "#6eaa49");
        vars.set_text("excludeBkgColor", "#eeeeee");
        vars.set_text("taskBkgColor", "#487e3a");
        vars.set_text("taskTextLightColor", "white");
        vars.set_text("taskTextDarkColor", "black");
        vars.set_text("taskTextClickableColor", "#003163");
        vars.set_text("gridColor", "lightgrey");
        vars.set_text("doneTaskBkgColor", "lightgrey");
        vars.set_text("doneTaskBorderColor", "grey");
        vars.set_text("critBorderColor", "#ff8888");
        vars.set_text("critBkgColor", "red");
        vars.set_text("todayLineColor", "red");
        vars.set_text("vertLineColor", "#00BFFF");

        vars.set_text("personBorder", &primary_border);
        vars.set_text("personBkg", "#cde498");

        vars.set_text("archEdgeWidth", "3");
        vars.set_text("archGroupBorderColor", &primary_border);
        vars.set_text("archGroupBorderWidth", "2px");

        vars.set_text("noteFontWeight", "normal");
        vars.set_text("fontWeight", "normal");

        vars.set_text("labelColor", "black");
        vars.set_text("errorBkgColor", "#552222");
        vars.set_text("errorTextColor", "#552222");

        vars.set("useGradient", json!(true));
        vars.set_text("gradientStart", &primary_border);
        vars.set_text("gradientStop", &secondary_border);
        vars.set_text(
            "dropShadow",
            "drop-shadow( 1px 2px 2px rgba(185,185,185,0.5))",
        );
        Ok(vars)
    }

    pub(super) fn update_colors(vars: &mut Vars) -> Result<(), ThemeError> {
        vars.set("actorBorder", json!(vars.darken("mainBkg", 20.0)?));
        vars.set("actorBkg", vars.value("mainBkg"));
        vars.set("labelBoxBkgColor", vars.value("actorBkg"));
        vars.set("labelTextColor", vars.value("actorTextColor"));
        vars.set("loopTextColor", vars.value("actorTextColor"));
        vars.set("noteBorderColor", vars.value("border2"));
        vars.set("noteTextColor", vars.value("actorTextColor"));
        vars.set("actorLineColor", vars.value("actorBorder"));
        default_from!(vars, "rectBkgColor", "tertiaryColor");

        let limit = color_limit(vars);
        for i in 0..limit {
            let key = format!("cScale{i}");
            default_to!(
                vars,
                &key,
                match i {
                    0 => vars.value("primaryColor"),
                    1 => vars.value("secondaryColor"),
                    2 => vars.value("tertiaryColor"),
                    3 => json!(vars.adjust("primaryColor", &[("h", 30.0)])?),
                    4 => json!(vars.adjust("primaryColor", &[("h", 60.0)])?),
                    5 => json!(vars.adjust("primaryColor", &[("h", 90.0)])?),
                    6 => json!(vars.adjust("primaryColor", &[("h", 120.0)])?),
                    7 => json!(vars.adjust("primaryColor", &[("h", 150.0)])?),
                    8 => json!(vars.adjust("primaryColor", &[("h", 210.0)])?),
                    9 => json!(vars.adjust("primaryColor", &[("h", 270.0)])?),
                    10 => json!(vars.adjust("primaryColor", &[("h", 300.0)])?),
                    _ => json!(vars.adjust("primaryColor", &[("h", 330.0)])?),
                }
            );
        }
        default_to!(
            vars,
            "cScalePeer1",
            json!(vars.darken("secondaryColor", 45.0)?)
        );
        default_to!(
            vars,
            "cScalePeer2",
            json!(vars.darken("tertiaryColor", 40.0)?)
        );
        for i in 0..limit {
            let scale = format!("cScale{i}");
            let value = json!(vars.darken(&scale, 10.0)?);
            vars.set(&scale, value);
            let peer = format!("cScalePeer{i}");
            default_to!(vars, &peer, json!(vars.darken(&scale, 25.0)?));
        }
        for i in 0..limit {
            let key = format!("cScaleInv{i}");
            let scale = format!("cScale{i}");
            default_to!(vars, &key, json!(vars.adjust(&scale, &[("h", 180.0)])?));
        }

        default_to!(
            vars,
            "scaleLabelColor",
            if vars.truthy("scaleLabelColor") && vars.text("scaleLabelColor") != Some("calculated")
            {
                vars.value("scaleLabelColor")
            } else {
                vars.value("labelTextColor")
            }
        );
        for i in 0..limit {
            let key = format!("cScaleLabel{i}");
            default_from!(vars, &key, "scaleLabelColor");
        }

        for i in 0..5 {
            let key = format!("surface{i}");
            default_to!(
                vars,
                &key,
                json!(vars.adjust(
                    "mainBkg",
                    &[("h", 30.0), ("s", -30.0), ("l", -(5.0 + i as f64 * 5.0))]
                )?)
            );
            let key = format!("surfacePeer{i}");
            default_to!(
                vars,
                &key,
                json!(vars.adjust(
                    "mainBkg",
                    &[("h", 30.0), ("s", -30.0), ("l", -(8.0 + i as f64 * 5.0))]
                )?)
            );
        }

        vars.set("nodeBkg", vars.value("mainBkg"));
        vars.set("nodeBorder", vars.value("border1"));
        vars.set("clusterBkg", vars.value("secondBkg"));
        vars.set("clusterBorder", vars.value("border2"));
        vars.set("defaultLinkColor", vars.value("lineColor"));

        default_from!(vars, "flowContainerStroke", "secondaryBorderColor");

        vars.set("taskBorderColor", vars.value("border1"));
        vars.set("taskTextColor", vars.value("taskTextLightColor"));
        vars.set("taskTextOutsideColor", vars.value("taskTextDarkColor"));
        vars.set("activeTaskBorderColor", vars.value("taskBorderColor"));
        vars.set("activeTaskBkgColor", vars.value("mainBkg"));

        vars.set("archEdgeColor", vars.value("lineColor"));
        vars.set("archEdgeArrowColor", vars.value("lineColor"));

        default_to!(vars, "rowOdd", json!(vars.lighten("mainBkg", 75.0)?));
        default_to!(vars, "rowEven", json!(vars.lighten("mainBkg", 20.0)?));

        default_from!(vars, "transitionColor", "lineColor");
        default_from!(vars, "transitionLabelColor", "textColor");
        default_to!(
            vars,
            "stateLabelColor",
            if vars.truthy("stateBkg") {
                vars.value("stateBkg")
            } else {
                vars.value("primaryTextColor")
            }
        );
        default_from!(vars, "stateBkg", "mainBkg");
        default_from!(vars, "labelBackgroundColor", "stateBkg");
        default_from!(vars, "compositeBackground", "background");
        default_to!(vars, "altBackground", json!("#f0f0f0"));
        default_from!(vars, "compositeTitleBackground", "mainBkg");
        default_from!(vars, "compositeBorder", "nodeBorder");
        vars.set("innerEndBackground", vars.value("primaryBorderColor"));
        vars.set("specialStateColor", vars.value("lineColor"));

        default_from!(vars, "errorBkgColor", "tertiaryColor");
        default_from!(vars, "errorTextColor", "tertiaryTextColor");
        default_from!(vars, "transitionColor", "lineColor");

        vars.set("classText", vars.value("primaryTextColor"));

        vars.set("fillType0", vars.value("primaryColor"));
        vars.set("fillType1", vars.value("secondaryColor"));
        vars.set(
            "fillType2",
            json!(vars.adjust("primaryColor", &[("h", 64.0)])?),
        );
        vars.set(
            "fillType3",
            json!(vars.adjust("secondaryColor", &[("h", 64.0)])?),
        );
        vars.set(
            "fillType4",
            json!(vars.adjust("primaryColor", &[("h", -64.0)])?),
        );
        vars.set(
            "fillType5",
            json!(vars.adjust("secondaryColor", &[("h", -64.0)])?),
        );
        vars.set(
            "fillType6",
            json!(vars.adjust("primaryColor", &[("h", 128.0)])?),
        );
        vars.set(
            "fillType7",
            json!(vars.adjust("secondaryColor", &[("h", 128.0)])?),
        );

        default_from!(vars, "pie1", "primaryColor");
        default_from!(vars, "pie2", "secondaryColor");
        default_from!(vars, "pie3", "tertiaryColor");
        default_to!(
            vars,
            "pie4",
            json!(vars.adjust("primaryColor", &[("l", -30.0)])?)
        );
        default_to!(
            vars,
            "pie5",
            json!(vars.adjust("secondaryColor", &[("l", -30.0)])?)
        );
        default_to!(
            vars,
            "pie6",
            json!(vars.adjust("tertiaryColor", &[("h", 40.0), ("l", -40.0)])?)
        );
        default_to!(
            vars,
            "pie7",
            json!(vars.adjust("primaryColor", &[("h", 60.0), ("l", -10.0)])?)
        );
        default_to!(
            vars,
            "pie8",
            json!(vars.adjust("primaryColor", &[("h", -60.0), ("l", -10.0)])?)
        );
        default_to!(
            vars,
            "pie9",
            json!(vars.adjust("primaryColor", &[("h", 120.0), ("l", 0.0)])?)
        );
        default_to!(
            vars,
            "pie10",
            json!(vars.adjust("primaryColor", &[("h", 60.0), ("l", -50.0)])?)
        );
        default_to!(
            vars,
            "pie11",
            json!(vars.adjust("primaryColor", &[("h", -60.0), ("l", -50.0)])?)
        );
        default_to!(
            vars,
            "pie12",
            json!(vars.adjust("primaryColor", &[("h", 120.0), ("l", -50.0)])?)
        );
        default_to!(vars, "pieTitleTextSize", json!("25px"));
        default_from!(vars, "pieTitleTextColor", "taskTextDarkColor");
        default_to!(vars, "pieSectionTextSize", json!("17px"));
        default_from!(vars, "pieSectionTextColor", "textColor");
        default_to!(vars, "pieLegendTextSize", json!("17px"));
        default_from!(vars, "pieLegendTextColor", "taskTextDarkColor");
        default_to!(vars, "pieStrokeColor", json!("black"));
        default_to!(vars, "pieStrokeWidth", json!("2px"));
        default_to!(vars, "pieOuterStrokeWidth", json!("2px"));
        default_to!(vars, "pieOuterStrokeColor", json!("black"));
        default_to!(vars, "pieOpacity", json!("0.7"));

        default_nullish!(
            vars,
            "venn1",
            json!(vars.adjust("primaryColor", &[("l", -30.0)])?)
        );
        default_nullish!(
            vars,
            "venn2",
            json!(vars.adjust("secondaryColor", &[("l", -30.0)])?)
        );
        default_nullish!(
            vars,
            "venn3",
            json!(vars.adjust("tertiaryColor", &[("l", -30.0)])?)
        );
        default_nullish!(
            vars,
            "venn4",
            json!(vars.adjust("primaryColor", &[("h", 60.0), ("l", -30.0)])?)
        );
        default_nullish!(
            vars,
            "venn5",
            json!(vars.adjust("primaryColor", &[("h", -60.0), ("l", -30.0)])?)
        );
        default_nullish!(
            vars,
            "venn6",
            json!(vars.adjust("secondaryColor", &[("h", 60.0), ("l", -30.0)])?)
        );
        default_nullish!(
            vars,
            "venn7",
            json!(vars.adjust("primaryColor", &[("h", 120.0), ("l", -30.0)])?)
        );
        default_nullish!(
            vars,
            "venn8",
            json!(vars.adjust("secondaryColor", &[("h", 120.0), ("l", -30.0)])?)
        );
        default_nullish!(vars, "vennTitleTextColor", vars.value("titleColor"));
        default_nullish!(vars, "vennSetTextColor", vars.value("textColor"));

        vars.set(
            "cynefin",
            object(vec![
                (
                    "domainFontSize",
                    vars.nested_or_else("cynefin", "domainFontSize", || json!(16)),
                ),
                (
                    "itemFontSize",
                    vars.nested_or_else("cynefin", "itemFontSize", || json!(12)),
                ),
                (
                    "boundaryColor",
                    vars.nested_or("cynefin", "boundaryColor", "lineColor"),
                ),
                (
                    "boundaryWidth",
                    vars.nested_or_else("cynefin", "boundaryWidth", || json!(2)),
                ),
                (
                    "cliffColor",
                    vars.nested_or_else("cynefin", "cliffColor", || json!("#8B4513")),
                ),
                (
                    "cliffWidth",
                    vars.nested_or_else("cynefin", "cliffWidth", || json!(4)),
                ),
                (
                    "arrowColor",
                    vars.nested_or("cynefin", "arrowColor", "lineColor"),
                ),
                (
                    "arrowWidth",
                    vars.nested_or_else("cynefin", "arrowWidth", || json!(2)),
                ),
                (
                    "complexBg",
                    vars.nested_or_else("cynefin", "complexBg", || json!("#C8E6C9")),
                ),
                (
                    "complicatedBg",
                    vars.nested_or_else("cynefin", "complicatedBg", || json!("#DCEDC8")),
                ),
                (
                    "chaoticBg",
                    vars.nested_or_else("cynefin", "chaoticBg", || json!("#FFE0B2")),
                ),
                (
                    "clearBg",
                    vars.nested_or_else("cynefin", "clearBg", || json!("#FFF9C4")),
                ),
                (
                    "confusionBg",
                    vars.nested_or_else("cynefin", "confusionBg", || json!("#D7CCC8")),
                ),
                (
                    "textColor",
                    vars.nested_or("cynefin", "textColor", "textColor"),
                ),
                (
                    "labelColor",
                    vars.nested_or("cynefin", "labelColor", "primaryTextColor"),
                ),
            ]),
        );

        default_from!(vars, "quadrant1Fill", "primaryColor");
        default_to!(
            vars,
            "quadrant2Fill",
            json!(vars.adjust("primaryColor", &[("r", 5.0), ("g", 5.0), ("b", 5.0)])?)
        );
        default_to!(
            vars,
            "quadrant3Fill",
            json!(vars.adjust("primaryColor", &[("r", 10.0), ("g", 10.0), ("b", 10.0)])?)
        );
        default_to!(
            vars,
            "quadrant4Fill",
            json!(vars.adjust("primaryColor", &[("r", 15.0), ("g", 15.0), ("b", 15.0)])?)
        );
        default_from!(vars, "quadrant1TextFill", "primaryTextColor");
        default_to!(
            vars,
            "quadrant2TextFill",
            json!(vars.adjust("primaryTextColor", &[("r", -5.0), ("g", -5.0), ("b", -5.0)])?)
        );
        default_to!(
            vars,
            "quadrant3TextFill",
            json!(vars.adjust(
                "primaryTextColor",
                &[("r", -10.0), ("g", -10.0), ("b", -10.0)]
            )?)
        );
        default_to!(
            vars,
            "quadrant4TextFill",
            json!(vars.adjust(
                "primaryTextColor",
                &[("r", -15.0), ("g", -15.0), ("b", -15.0)]
            )?)
        );
        default_quadrant_point_fill(vars)?;
        default_from!(vars, "quadrantPointTextFill", "primaryTextColor");
        default_from!(vars, "quadrantXAxisTextFill", "primaryTextColor");
        default_from!(vars, "quadrantYAxisTextFill", "primaryTextColor");
        default_from!(
            vars,
            "quadrantInternalBorderStrokeFill",
            "primaryBorderColor"
        );
        default_from!(
            vars,
            "quadrantExternalBorderStrokeFill",
            "primaryBorderColor"
        );
        default_from!(vars, "quadrantTitleFill", "primaryTextColor");

        vars.set(
            "packet",
            object(vec![
                ("startByteColor", vars.value("primaryTextColor")),
                ("endByteColor", vars.value("primaryTextColor")),
                ("labelColor", vars.value("primaryTextColor")),
                ("titleColor", vars.value("primaryTextColor")),
                ("blockStrokeColor", vars.value("primaryTextColor")),
                ("blockFillColor", vars.value("mainBkg")),
            ]),
        );

        vars.set(
            "radar",
            object(vec![
                (
                    "axisColor",
                    vars.nested_or("radar", "axisColor", "lineColor"),
                ),
                (
                    "axisStrokeWidth",
                    vars.nested_or_else("radar", "axisStrokeWidth", || json!(2)),
                ),
                (
                    "axisLabelFontSize",
                    vars.nested_or_else("radar", "axisLabelFontSize", || json!(12)),
                ),
                (
                    "curveOpacity",
                    vars.nested_or_else("radar", "curveOpacity", || json!(0.5)),
                ),
                (
                    "curveStrokeWidth",
                    vars.nested_or_else("radar", "curveStrokeWidth", || json!(2)),
                ),
                (
                    "graticuleColor",
                    vars.nested_or_else("radar", "graticuleColor", || json!("#DEDEDE")),
                ),
                (
                    "graticuleStrokeWidth",
                    vars.nested_or_else("radar", "graticuleStrokeWidth", || json!(1)),
                ),
                (
                    "graticuleOpacity",
                    vars.nested_or_else("radar", "graticuleOpacity", || json!(0.3)),
                ),
                (
                    "legendBoxSize",
                    vars.nested_or_else("radar", "legendBoxSize", || json!(12)),
                ),
                (
                    "legendFontSize",
                    vars.nested_or_else("radar", "legendFontSize", || json!(12)),
                ),
            ]),
        );

        default_to!(vars, "wardleyEvolutionColor", json!("#dc3545"));
        vars.set(
            "wardley",
            object(vec![
                (
                    "backgroundColor",
                    vars.nested_or("wardley", "backgroundColor", "background"),
                ),
                (
                    "axisColor",
                    vars.nested_or("wardley", "axisColor", "lineColor"),
                ),
                (
                    "axisTextColor",
                    vars.nested_or("wardley", "axisTextColor", "primaryTextColor"),
                ),
                (
                    "gridColor",
                    vars.nested_or("wardley", "gridColor", "gridColor"),
                ),
                (
                    "componentFill",
                    vars.nested_or("wardley", "componentFill", "background"),
                ),
                (
                    "componentStroke",
                    vars.nested_or("wardley", "componentStroke", "lineColor"),
                ),
                (
                    "componentLabelColor",
                    vars.nested_or("wardley", "componentLabelColor", "primaryTextColor"),
                ),
                (
                    "linkStroke",
                    vars.nested_or("wardley", "linkStroke", "lineColor"),
                ),
                (
                    "evolutionStroke",
                    vars.nested_or("wardley", "evolutionStroke", "wardleyEvolutionColor"),
                ),
                (
                    "annotationStroke",
                    vars.nested_or("wardley", "annotationStroke", "lineColor"),
                ),
                (
                    "annotationTextColor",
                    vars.nested_or("wardley", "annotationTextColor", "primaryTextColor"),
                ),
                (
                    "annotationFill",
                    vars.nested_or("wardley", "annotationFill", "background"),
                ),
            ]),
        );

        vars.set(
            "xyChart",
            object(vec![
                (
                    "backgroundColor",
                    vars.nested_or("xyChart", "backgroundColor", "background"),
                ),
                (
                    "titleColor",
                    vars.nested_or("xyChart", "titleColor", "primaryTextColor"),
                ),
                (
                    "dataLabelColor",
                    vars.nested_or("xyChart", "dataLabelColor", "primaryTextColor"),
                ),
                (
                    "legendTextColor",
                    vars.nested_or("xyChart", "legendTextColor", "primaryTextColor"),
                ),
                (
                    "xAxisTitleColor",
                    vars.nested_or("xyChart", "xAxisTitleColor", "primaryTextColor"),
                ),
                (
                    "xAxisLabelColor",
                    vars.nested_or("xyChart", "xAxisLabelColor", "primaryTextColor"),
                ),
                (
                    "xAxisTickColor",
                    vars.nested_or("xyChart", "xAxisTickColor", "primaryTextColor"),
                ),
                (
                    "xAxisLineColor",
                    vars.nested_or("xyChart", "xAxisLineColor", "primaryTextColor"),
                ),
                (
                    "yAxisTitleColor",
                    vars.nested_or("xyChart", "yAxisTitleColor", "primaryTextColor"),
                ),
                (
                    "yAxisLabelColor",
                    vars.nested_or("xyChart", "yAxisLabelColor", "primaryTextColor"),
                ),
                (
                    "yAxisTickColor",
                    vars.nested_or("xyChart", "yAxisTickColor", "primaryTextColor"),
                ),
                (
                    "yAxisLineColor",
                    vars.nested_or("xyChart", "yAxisLineColor", "primaryTextColor"),
                ),
                (
                    "plotColorPalette",
                    vars.nested_or_else("xyChart", "plotColorPalette", || {
                        json!(
                            "#CDE498,#FF6B6B,#A0D2DB,#D7BDE2,#F0F0F0,#FFC3A0,#7FD8BE,#FF9A8B,#FAF3E0,#FFF176"
                        )
                    }),
                ),
            ]),
        );

        default_from!(vars, "requirementBackground", "primaryColor");
        default_from!(vars, "requirementBorderColor", "primaryBorderColor");
        default_to!(vars, "requirementBorderSize", json!("1"));
        default_from!(vars, "requirementTextColor", "primaryTextColor");
        default_from!(vars, "relationColor", "lineColor");
        default_from!(vars, "relationLabelBackground", "edgeLabelBackground");
        default_from!(vars, "relationLabelColor", "actorTextColor");

        default_from!(vars, "git0", "primaryColor");
        default_from!(vars, "git1", "secondaryColor");
        default_from!(vars, "git2", "tertiaryColor");
        default_to!(
            vars,
            "git3",
            json!(vars.adjust("primaryColor", &[("h", -30.0)])?)
        );
        default_to!(
            vars,
            "git4",
            json!(vars.adjust("primaryColor", &[("h", -60.0)])?)
        );
        default_to!(
            vars,
            "git5",
            json!(vars.adjust("primaryColor", &[("h", -90.0)])?)
        );
        default_to!(
            vars,
            "git6",
            json!(vars.adjust("primaryColor", &[("h", 60.0)])?)
        );
        default_to!(
            vars,
            "git7",
            json!(vars.adjust("primaryColor", &[("h", 120.0)])?)
        );
        for i in 0..8 {
            let key = format!("git{i}");
            let value = if vars.dark_mode() {
                json!(vars.lighten(&key, 25.0)?)
            } else {
                json!(vars.darken(&key, 25.0)?)
            };
            vars.set(&key, value);
        }
        for i in 0..8 {
            let key = format!("gitInv{i}");
            let source = format!("git{i}");
            default_to!(vars, &key, json!(vars.invert(&source)?));
        }
        default_to!(
            vars,
            "gitBranchLabel0",
            json!(vars.invert("labelTextColor")?)
        );
        default_from!(vars, "gitBranchLabel1", "labelTextColor");
        default_from!(vars, "gitBranchLabel2", "labelTextColor");
        default_to!(
            vars,
            "gitBranchLabel3",
            json!(vars.invert("labelTextColor")?)
        );
        default_from!(vars, "gitBranchLabel4", "labelTextColor");
        default_from!(vars, "gitBranchLabel5", "labelTextColor");
        default_from!(vars, "gitBranchLabel6", "labelTextColor");
        default_from!(vars, "gitBranchLabel7", "labelTextColor");

        default_from!(vars, "tagLabelColor", "primaryTextColor");
        default_from!(vars, "tagLabelBackground", "primaryColor");
        default_to!(
            vars,
            "tagLabelBorder",
            if vars.truthy("tagBorder") {
                vars.value("tagBorder")
            } else {
                vars.value("primaryBorderColor")
            }
        );
        default_to!(vars, "tagLabelFontSize", json!("10px"));
        default_from!(vars, "commitLabelColor", "secondaryTextColor");
        default_from!(vars, "commitLabelBackground", "secondaryColor");
        default_to!(vars, "commitLabelFontSize", json!("10px"));

        default_to!(vars, "emUiFill", json!("white"));
        default_to!(vars, "emUiStroke", json!("#dbdada"));
        default_to!(vars, "emProcessorFill", json!("#edb3f6"));
        default_to!(vars, "emProcessorStroke", json!("#b88cbf"));
        default_to!(vars, "emReadModelFill", json!("#d3f1a2"));
        default_to!(vars, "emReadModelStroke", json!("#a3b732"));
        default_to!(vars, "emCommandFill", json!("#bcd6fe"));
        default_to!(vars, "emCommandStroke", json!("#679ac3"));
        default_to!(vars, "emEventFill", json!("#ffb778"));
        default_to!(vars, "emEventStroke", json!("#c19a0f"));
        default_to!(vars, "emSwimlaneBackgroundOdd", json!("rgb(250,250,250)"));
        default_to!(
            vars,
            "emSwimlaneBackgroundStroke",
            json!("rgb(240,240,240)")
        );
        default_from!(vars, "emArrowhead", "lineColor");
        default_from!(vars, "emRelationStroke", "lineColor");

        default_to!(
            vars,
            "attributeBackgroundColorOdd",
            json!(OLD_ATTRIBUTE_ODD)
        );
        default_to!(
            vars,
            "attributeBackgroundColorEven",
            json!(OLD_ATTRIBUTE_EVEN)
        );
        Ok(())
    }

    fn color_limit(vars: &Vars) -> usize {
        vars.values
            .get("THEME_COLOR_LIMIT")
            .and_then(Value::as_f64)
            .unwrap_or(12.0) as usize
    }
}
mod neutral_theme {
    use super::*;

    const OLD_ATTRIBUTE_ODD: &str = "#ffffff";
    const OLD_ATTRIBUTE_EVEN: &str = "#f2f2f2";

    pub(super) fn constructor() -> Result<Vars, ThemeError> {
        let mut vars = Vars::new();
        vars.set_text("primaryColor", "#eee");
        vars.set_text("contrast", "#707070");
        vars.set_text("secondaryColor", &color::lighten("#707070", 55.0)?);
        vars.set_text("background", "#ffffff");
        vars.set_text("tertiaryColor", &color::adjust("#eee", &[("h", -160.0)])?);
        let primary_border = color::adjust("#eee", &[("s", -40.0), ("l", -10.0)])?;
        let secondary_border = color::adjust(
            &color::lighten("#707070", 55.0)?,
            &[("s", -40.0), ("l", -10.0)],
        )?;
        let tertiary_border = color::adjust(
            &color::adjust("#eee", &[("h", -160.0)])?,
            &[("s", -40.0), ("l", -10.0)],
        )?;
        vars.set_text("primaryBorderColor", &primary_border);
        vars.set_text("secondaryBorderColor", &secondary_border);
        vars.set_text("tertiaryBorderColor", &tertiary_border);
        vars.set_text("primaryTextColor", &color::invert("#eee", 100.0)?);
        vars.set_text(
            "secondaryTextColor",
            &color::invert(&color::lighten("#707070", 55.0)?, 100.0)?,
        );
        vars.set_text(
            "tertiaryTextColor",
            &color::invert(&color::adjust("#eee", &[("h", -160.0)])?, 100.0)?,
        );
        vars.set_text("lineColor", &color::invert("#ffffff", 100.0)?);
        vars.set_text("textColor", &color::invert("#ffffff", 100.0)?);

        vars.set_text("mainBkg", "#eee");
        vars.set_text("lineColor", "#666");
        vars.set_text("border1", "#999");
        vars.set_text("note", "#ffa");
        vars.set_text("text", "#333");
        vars.set_text("critical", "#d42");
        vars.set_text("done", "#bbb");
        vars.set_text("arrowheadColor", "#333333");
        vars.set_text("fontFamily", "\"trebuchet ms\", verdana, arial, sans-serif");
        vars.set_text("fontSize", "16px");
        vars.set("THEME_COLOR_LIMIT", json!(12));
        vars.set("radius", json!(5));
        vars.set("strokeWidth", json!(1));

        vars.set_text("edgeLabelBackground", "white");

        vars.set_text("activationBorderColor", "#666");
        vars.set_text("activationBkgColor", "#f4f4f4");
        vars.set_text("sequenceNumberColor", "white");

        vars.set_text("altSectionBkgColor", "white");
        vars.set_text("excludeBkgColor", "#eeeeee");
        vars.set_text("taskTextLightColor", "white");
        vars.set_text("taskTextClickableColor", "#003163");

        vars.set_text("personBorder", &primary_border);
        vars.set_text("personBkg", "#eee");

        vars.set_text("archEdgeWidth", "3");
        vars.set_text("archGroupBorderColor", &primary_border);
        vars.set_text("archGroupBorderWidth", "2px");

        vars.set_text("noteFontWeight", "normal");
        vars.set_text("fontWeight", "normal");

        vars.set_text("rowOdd", &color::lighten("#eee", 75.0)?);
        vars.set_text("rowEven", "#f4f4f4");

        vars.set_text("labelColor", "black");
        vars.set_text("errorBkgColor", "#552222");
        vars.set_text("errorTextColor", "#552222");

        vars.set("useGradient", json!(true));
        vars.set_text("gradientStart", &primary_border);
        vars.set_text("gradientStop", &secondary_border);
        vars.set_text(
            "dropShadow",
            "drop-shadow( 1px 2px 2px rgba(185,185,185,1))",
        );
        Ok(vars)
    }

    pub(super) fn update_colors(vars: &mut Vars) -> Result<(), ThemeError> {
        vars.set("secondBkg", json!(vars.lighten("contrast", 55.0)?));
        vars.set("border2", vars.value("contrast"));

        vars.set("actorBorder", json!(vars.lighten("border1", 23.0)?));
        vars.set("actorBkg", vars.value("mainBkg"));
        vars.set("actorTextColor", vars.value("text"));
        vars.set("actorLineColor", vars.value("actorBorder"));
        default_from!(vars, "rectBkgColor", "tertiaryColor");
        vars.set("signalColor", vars.value("text"));
        vars.set("signalTextColor", vars.value("text"));
        vars.set("labelBoxBkgColor", vars.value("actorBkg"));
        vars.set("labelBoxBorderColor", vars.value("actorBorder"));
        vars.set("labelTextColor", vars.value("text"));
        vars.set("loopTextColor", vars.value("text"));
        vars.set_text("noteBorderColor", "#999");
        vars.set_text("noteBkgColor", "#666");
        vars.set_text("noteTextColor", "#fff");

        let scale = [
            "#555", "#F4F4F4", "#555", "#BBB", "#777", "#999", "#DDD", "#FFF", "#DDD", "#BBB",
            "#999", "#777",
        ];
        let limit = color_limit(vars);
        for (i, value) in scale.iter().enumerate().take(limit) {
            default_to!(vars, &format!("cScale{i}"), json!(value));
        }
        for i in 0..limit {
            let key = format!("cScaleInv{i}");
            let scale = format!("cScale{i}");
            default_to!(vars, &key, json!(vars.invert(&scale)?));
        }
        for i in 0..limit {
            let key = format!("cScalePeer{i}");
            let scale = format!("cScale{i}");
            let value = if vars.dark_mode() {
                json!(vars.lighten(&scale, 10.0)?)
            } else {
                json!(vars.darken(&scale, 10.0)?)
            };
            default_to!(vars, &key, value);
        }

        default_to!(
            vars,
            "scaleLabelColor",
            if vars.dark_mode() {
                json!("black")
            } else {
                vars.value("labelTextColor")
            }
        );
        default_from!(vars, "cScaleLabel0", "cScale1");
        default_from!(vars, "cScaleLabel2", "cScale1");
        for i in 0..limit {
            let key = format!("cScaleLabel{i}");
            default_from!(vars, &key, "scaleLabelColor");
        }

        for i in 0..5 {
            let key = format!("surface{i}");
            default_to!(
                vars,
                &key,
                json!(vars.adjust("mainBkg", &[("l", -(5.0 + i as f64 * 5.0))])?)
            );
            let key = format!("surfacePeer{i}");
            default_to!(
                vars,
                &key,
                json!(vars.adjust("mainBkg", &[("l", -(8.0 + i as f64 * 5.0))])?)
            );
        }

        vars.set("nodeBkg", vars.value("mainBkg"));
        vars.set("nodeBorder", vars.value("border1"));
        vars.set("clusterBkg", vars.value("secondBkg"));
        vars.set("clusterBorder", vars.value("border2"));
        vars.set("defaultLinkColor", vars.value("lineColor"));
        vars.set("titleColor", vars.value("text"));

        default_from!(vars, "flowContainerStroke", "secondaryBorderColor");

        vars.set("sectionBkgColor", json!(vars.lighten("contrast", 30.0)?));
        vars.set("sectionBkgColor2", json!(vars.lighten("contrast", 30.0)?));
        vars.set("taskBorderColor", json!(vars.darken("contrast", 10.0)?));
        vars.set("taskBkgColor", vars.value("contrast"));
        vars.set("taskTextColor", vars.value("taskTextLightColor"));
        vars.set("taskTextDarkColor", vars.value("text"));
        vars.set("taskTextOutsideColor", vars.value("taskTextDarkColor"));
        vars.set("activeTaskBorderColor", vars.value("taskBorderColor"));
        vars.set("activeTaskBkgColor", vars.value("mainBkg"));
        vars.set("gridColor", json!(vars.lighten("border1", 30.0)?));
        vars.set("doneTaskBkgColor", vars.value("done"));
        vars.set("doneTaskBorderColor", vars.value("lineColor"));
        vars.set("critBkgColor", vars.value("critical"));
        vars.set("critBorderColor", json!(vars.darken("critBkgColor", 10.0)?));
        vars.set("todayLineColor", vars.value("critBkgColor"));
        vars.set("vertLineColor", vars.value("critBkgColor"));

        vars.set("archEdgeColor", vars.value("lineColor"));
        vars.set("archEdgeArrowColor", vars.value("lineColor"));

        default_to!(vars, "transitionColor", json!("#000"));
        default_from!(vars, "transitionLabelColor", "textColor");
        default_to!(
            vars,
            "stateLabelColor",
            if vars.truthy("stateBkg") {
                vars.value("stateBkg")
            } else {
                vars.value("primaryTextColor")
            }
        );
        default_from!(vars, "stateBkg", "mainBkg");
        default_from!(vars, "labelBackgroundColor", "stateBkg");
        default_from!(vars, "compositeBackground", "background");
        default_to!(vars, "altBackground", json!("#f4f4f4"));
        default_from!(vars, "compositeTitleBackground", "mainBkg");
        default_to!(vars, "stateBorder", json!("#000"));
        vars.set("innerEndBackground", vars.value("primaryBorderColor"));
        vars.set_text("specialStateColor", "#222");

        default_from!(vars, "errorBkgColor", "tertiaryColor");
        default_from!(vars, "errorTextColor", "tertiaryTextColor");

        vars.set("classText", vars.value("primaryTextColor"));
        vars.set("fillType0", vars.value("primaryColor"));
        vars.set("fillType1", vars.value("secondaryColor"));
        vars.set(
            "fillType2",
            json!(vars.adjust("primaryColor", &[("h", 64.0)])?),
        );
        vars.set(
            "fillType3",
            json!(vars.adjust("secondaryColor", &[("h", 64.0)])?),
        );
        vars.set(
            "fillType4",
            json!(vars.adjust("primaryColor", &[("h", -64.0)])?),
        );
        vars.set(
            "fillType5",
            json!(vars.adjust("secondaryColor", &[("h", -64.0)])?),
        );
        vars.set(
            "fillType6",
            json!(vars.adjust("primaryColor", &[("h", 128.0)])?),
        );
        vars.set(
            "fillType7",
            json!(vars.adjust("secondaryColor", &[("h", 128.0)])?),
        );

        for i in 0..limit {
            let pie = format!("pie{i}");
            let scale = format!("cScale{i}");
            vars.set(&pie, vars.value(&scale));
        }
        vars.set("pie12", vars.value("pie0"));
        default_to!(vars, "pieTitleTextSize", json!("25px"));
        default_from!(vars, "pieTitleTextColor", "taskTextDarkColor");
        default_to!(vars, "pieSectionTextSize", json!("17px"));
        default_from!(vars, "pieSectionTextColor", "textColor");
        default_to!(vars, "pieLegendTextSize", json!("17px"));
        default_from!(vars, "pieLegendTextColor", "taskTextDarkColor");
        default_to!(vars, "pieStrokeColor", json!("black"));
        default_to!(vars, "pieStrokeWidth", json!("2px"));
        default_to!(vars, "pieOuterStrokeWidth", json!("2px"));
        default_to!(vars, "pieOuterStrokeColor", json!("black"));
        default_to!(vars, "pieOpacity", json!("0.7"));

        for i in 0..8 {
            let venn = format!("venn{}", i + 1);
            let scale = format!("cScale{i}");
            default_nullish!(vars, &venn, vars.value(&scale));
        }
        default_nullish!(vars, "vennTitleTextColor", vars.value("titleColor"));
        default_nullish!(vars, "vennSetTextColor", vars.value("textColor"));

        vars.set(
            "cynefin",
            object(vec![
                (
                    "domainFontSize",
                    vars.nested_or_else("cynefin", "domainFontSize", || json!(16)),
                ),
                (
                    "itemFontSize",
                    vars.nested_or_else("cynefin", "itemFontSize", || json!(12)),
                ),
                (
                    "boundaryColor",
                    vars.nested_or("cynefin", "boundaryColor", "lineColor"),
                ),
                (
                    "boundaryWidth",
                    vars.nested_or_else("cynefin", "boundaryWidth", || json!(2)),
                ),
                (
                    "cliffColor",
                    vars.nested_or_else("cynefin", "cliffColor", || json!("#8B0000")),
                ),
                (
                    "cliffWidth",
                    vars.nested_or_else("cynefin", "cliffWidth", || json!(4)),
                ),
                (
                    "arrowColor",
                    vars.nested_or("cynefin", "arrowColor", "lineColor"),
                ),
                (
                    "arrowWidth",
                    vars.nested_or_else("cynefin", "arrowWidth", || json!(2)),
                ),
                (
                    "complexBg",
                    vars.nested_or_else("cynefin", "complexBg", || json!("#E8F5E9")),
                ),
                (
                    "complicatedBg",
                    vars.nested_or_else("cynefin", "complicatedBg", || json!("#E3F2FD")),
                ),
                (
                    "chaoticBg",
                    vars.nested_or_else("cynefin", "chaoticBg", || json!("#FBE9E7")),
                ),
                (
                    "clearBg",
                    vars.nested_or_else("cynefin", "clearBg", || json!("#FFF8E1")),
                ),
                (
                    "confusionBg",
                    vars.nested_or_else("cynefin", "confusionBg", || json!("#F3E5F5")),
                ),
                (
                    "textColor",
                    vars.nested_or("cynefin", "textColor", "textColor"),
                ),
                (
                    "labelColor",
                    vars.nested_or("cynefin", "labelColor", "primaryTextColor"),
                ),
            ]),
        );

        default_from!(vars, "quadrant1Fill", "primaryColor");
        default_to!(
            vars,
            "quadrant2Fill",
            json!(vars.adjust("primaryColor", &[("r", 5.0), ("g", 5.0), ("b", 5.0)])?)
        );
        default_to!(
            vars,
            "quadrant3Fill",
            json!(vars.adjust("primaryColor", &[("r", 10.0), ("g", 10.0), ("b", 10.0)])?)
        );
        default_to!(
            vars,
            "quadrant4Fill",
            json!(vars.adjust("primaryColor", &[("r", 15.0), ("g", 15.0), ("b", 15.0)])?)
        );
        default_from!(vars, "quadrant1TextFill", "primaryTextColor");
        default_to!(
            vars,
            "quadrant2TextFill",
            json!(vars.adjust("primaryTextColor", &[("r", -5.0), ("g", -5.0), ("b", -5.0)])?)
        );
        default_to!(
            vars,
            "quadrant3TextFill",
            json!(vars.adjust(
                "primaryTextColor",
                &[("r", -10.0), ("g", -10.0), ("b", -10.0)]
            )?)
        );
        default_to!(
            vars,
            "quadrant4TextFill",
            json!(vars.adjust(
                "primaryTextColor",
                &[("r", -15.0), ("g", -15.0), ("b", -15.0)]
            )?)
        );
        default_quadrant_point_fill(vars)?;
        default_from!(vars, "quadrantPointTextFill", "primaryTextColor");
        default_from!(vars, "quadrantXAxisTextFill", "primaryTextColor");
        default_from!(vars, "quadrantYAxisTextFill", "primaryTextColor");
        default_from!(
            vars,
            "quadrantInternalBorderStrokeFill",
            "primaryBorderColor"
        );
        default_from!(
            vars,
            "quadrantExternalBorderStrokeFill",
            "primaryBorderColor"
        );
        default_from!(vars, "quadrantTitleFill", "primaryTextColor");

        vars.set(
            "xyChart",
            object(vec![
                (
                    "backgroundColor",
                    vars.nested_or("xyChart", "backgroundColor", "background"),
                ),
                (
                    "titleColor",
                    vars.nested_or("xyChart", "titleColor", "primaryTextColor"),
                ),
                (
                    "dataLabelColor",
                    vars.nested_or("xyChart", "dataLabelColor", "primaryTextColor"),
                ),
                (
                    "legendTextColor",
                    vars.nested_or("xyChart", "legendTextColor", "primaryTextColor"),
                ),
                (
                    "xAxisTitleColor",
                    vars.nested_or("xyChart", "xAxisTitleColor", "primaryTextColor"),
                ),
                (
                    "xAxisLabelColor",
                    vars.nested_or("xyChart", "xAxisLabelColor", "primaryTextColor"),
                ),
                (
                    "xAxisTickColor",
                    vars.nested_or("xyChart", "xAxisTickColor", "primaryTextColor"),
                ),
                (
                    "xAxisLineColor",
                    vars.nested_or("xyChart", "xAxisLineColor", "primaryTextColor"),
                ),
                (
                    "yAxisTitleColor",
                    vars.nested_or("xyChart", "yAxisTitleColor", "primaryTextColor"),
                ),
                (
                    "yAxisLabelColor",
                    vars.nested_or("xyChart", "yAxisLabelColor", "primaryTextColor"),
                ),
                (
                    "yAxisTickColor",
                    vars.nested_or("xyChart", "yAxisTickColor", "primaryTextColor"),
                ),
                (
                    "yAxisLineColor",
                    vars.nested_or("xyChart", "yAxisLineColor", "primaryTextColor"),
                ),
                (
                    "plotColorPalette",
                    vars.nested_or_else("xyChart", "plotColorPalette", || {
                        json!("#EEE,#6BB8E4,#8ACB88,#C7ACD6,#E8DCC2,#FFB2A8,#FFF380,#7E8D91,#FFD8B1,#FAF3E0")
                    }),
                ),
            ]),
        );

        vars.set(
            "radar",
            object(vec![
                (
                    "axisColor",
                    vars.nested_or("radar", "axisColor", "lineColor"),
                ),
                (
                    "axisStrokeWidth",
                    vars.nested_or_else("radar", "axisStrokeWidth", || json!(2)),
                ),
                (
                    "axisLabelFontSize",
                    vars.nested_or_else("radar", "axisLabelFontSize", || json!(12)),
                ),
                (
                    "curveOpacity",
                    vars.nested_or_else("radar", "curveOpacity", || json!(0.5)),
                ),
                (
                    "curveStrokeWidth",
                    vars.nested_or_else("radar", "curveStrokeWidth", || json!(2)),
                ),
                (
                    "graticuleColor",
                    vars.nested_or_else("radar", "graticuleColor", || json!("#DEDEDE")),
                ),
                (
                    "graticuleStrokeWidth",
                    vars.nested_or_else("radar", "graticuleStrokeWidth", || json!(1)),
                ),
                (
                    "graticuleOpacity",
                    vars.nested_or_else("radar", "graticuleOpacity", || json!(0.3)),
                ),
                (
                    "legendBoxSize",
                    vars.nested_or_else("radar", "legendBoxSize", || json!(12)),
                ),
                (
                    "legendFontSize",
                    vars.nested_or_else("radar", "legendFontSize", || json!(12)),
                ),
            ]),
        );

        default_to!(vars, "wardleyEvolutionColor", json!("#dc3545"));
        vars.set(
            "wardley",
            object(vec![
                (
                    "backgroundColor",
                    vars.nested_or("wardley", "backgroundColor", "background"),
                ),
                (
                    "axisColor",
                    vars.nested_or("wardley", "axisColor", "lineColor"),
                ),
                (
                    "axisTextColor",
                    vars.nested_or("wardley", "axisTextColor", "primaryTextColor"),
                ),
                (
                    "gridColor",
                    vars.nested_or("wardley", "gridColor", "gridColor"),
                ),
                (
                    "componentFill",
                    vars.nested_or("wardley", "componentFill", "background"),
                ),
                (
                    "componentStroke",
                    vars.nested_or("wardley", "componentStroke", "lineColor"),
                ),
                (
                    "componentLabelColor",
                    vars.nested_or("wardley", "componentLabelColor", "primaryTextColor"),
                ),
                (
                    "linkStroke",
                    vars.nested_or("wardley", "linkStroke", "lineColor"),
                ),
                (
                    "evolutionStroke",
                    vars.nested_or("wardley", "evolutionStroke", "wardleyEvolutionColor"),
                ),
                (
                    "annotationStroke",
                    vars.nested_or("wardley", "annotationStroke", "lineColor"),
                ),
                (
                    "annotationTextColor",
                    vars.nested_or("wardley", "annotationTextColor", "primaryTextColor"),
                ),
                (
                    "annotationFill",
                    vars.nested_or("wardley", "annotationFill", "background"),
                ),
            ]),
        );

        default_from!(vars, "requirementBackground", "primaryColor");
        default_from!(vars, "requirementBorderColor", "primaryBorderColor");
        default_to!(vars, "requirementBorderSize", json!("1"));
        default_from!(vars, "requirementTextColor", "primaryTextColor");
        default_from!(vars, "relationColor", "lineColor");
        default_from!(vars, "relationLabelBackground", "edgeLabelBackground");
        default_from!(vars, "relationLabelColor", "actorTextColor");

        vars.set("git0", json!(vars.darken("pie1", 25.0)?));
        default_to!(vars, "git1", vars.value("pie2"));
        default_to!(vars, "git2", vars.value("pie3"));
        default_to!(
            vars,
            "git3",
            if vars.truthy("pie4") {
                vars.value("pie4")
            } else {
                json!(vars.adjust("primaryColor", &[("h", -30.0)])?)
            }
        );
        default_to!(
            vars,
            "git4",
            if vars.truthy("pie5") {
                vars.value("pie5")
            } else {
                json!(vars.adjust("primaryColor", &[("h", -60.0)])?)
            }
        );
        default_to!(
            vars,
            "git5",
            if vars.truthy("pie6") {
                vars.value("pie6")
            } else {
                json!(vars.adjust("primaryColor", &[("h", -90.0)])?)
            }
        );
        default_to!(
            vars,
            "git6",
            if vars.truthy("pie7") {
                vars.value("pie7")
            } else {
                json!(vars.adjust("primaryColor", &[("h", 60.0)])?)
            }
        );
        default_to!(
            vars,
            "git7",
            if vars.truthy("pie8") {
                vars.value("pie8")
            } else {
                json!(vars.adjust("primaryColor", &[("h", 120.0)])?)
            }
        );
        for i in 0..8 {
            let key = format!("gitInv{i}");
            let source = format!("git{i}");
            default_to!(vars, &key, json!(vars.invert(&source)?));
        }

        default_from!(vars, "branchLabelColor", "labelTextColor");
        vars.set("gitBranchLabel0", vars.value("branchLabelColor"));
        vars.set_text("gitBranchLabel1", "white");
        vars.set("gitBranchLabel2", vars.value("branchLabelColor"));
        vars.set_text("gitBranchLabel3", "white");
        vars.set("gitBranchLabel4", vars.value("branchLabelColor"));
        vars.set("gitBranchLabel5", vars.value("branchLabelColor"));
        vars.set("gitBranchLabel6", vars.value("branchLabelColor"));
        vars.set("gitBranchLabel7", vars.value("branchLabelColor"));

        default_from!(vars, "tagLabelColor", "primaryTextColor");
        default_from!(vars, "tagLabelBackground", "primaryColor");
        default_to!(
            vars,
            "tagLabelBorder",
            if vars.truthy("tagBorder") {
                vars.value("tagBorder")
            } else {
                vars.value("primaryBorderColor")
            }
        );
        default_to!(vars, "tagLabelFontSize", json!("10px"));
        default_from!(vars, "commitLabelColor", "secondaryTextColor");
        default_from!(vars, "commitLabelBackground", "secondaryColor");
        default_to!(vars, "commitLabelFontSize", json!("10px"));

        default_to!(vars, "emUiFill", json!("white"));
        default_to!(vars, "emUiStroke", json!("#dbdada"));
        default_to!(vars, "emProcessorFill", json!("#edb3f6"));
        default_to!(vars, "emProcessorStroke", json!("#b88cbf"));
        default_to!(vars, "emReadModelFill", json!("#d3f1a2"));
        default_to!(vars, "emReadModelStroke", json!("#a3b732"));
        default_to!(vars, "emCommandFill", json!("#bcd6fe"));
        default_to!(vars, "emCommandStroke", json!("#679ac3"));
        default_to!(vars, "emEventFill", json!("#ffb778"));
        default_to!(vars, "emEventStroke", json!("#c19a0f"));
        default_to!(vars, "emSwimlaneBackgroundOdd", json!("rgb(250,250,250)"));
        default_to!(
            vars,
            "emSwimlaneBackgroundStroke",
            json!("rgb(240,240,240)")
        );
        default_from!(vars, "emArrowhead", "lineColor");
        default_from!(vars, "emRelationStroke", "lineColor");

        default_to!(
            vars,
            "attributeBackgroundColorOdd",
            json!(OLD_ATTRIBUTE_ODD)
        );
        default_to!(
            vars,
            "attributeBackgroundColorEven",
            json!(OLD_ATTRIBUTE_EVEN)
        );
        Ok(())
    }

    fn color_limit(vars: &Vars) -> usize {
        vars.values
            .get("THEME_COLOR_LIMIT")
            .and_then(Value::as_f64)
            .unwrap_or(12.0) as usize
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const DEFAULT_NONE: &str = r##"{"background":"white","primaryColor":"#ECECFF","secondaryColor":"#ffffde","tertiaryColor":"hsl(80, 100%, 96.2745098039%)","primaryBorderColor":"hsl(240, 60%, 86.2745098039%)","secondaryBorderColor":"hsl(60, 60%, 83.5294117647%)","tertiaryBorderColor":"hsl(80, 60%, 86.2745098039%)","primaryTextColor":"#131300","secondaryTextColor":"#000021","tertiaryTextColor":"rgb(6.3333333334, 0, 19.0000000001)","lineColor":"#333333","textColor":"#333","mainBkg":"#ECECFF","secondBkg":"#ffffde","border1":"#9370DB","border2":"#aaaa33","arrowheadColor":"#333333","fontFamily":"\"trebuchet ms\", verdana, arial, sans-serif","fontSize":"16px","labelBackground":"rgba(232,232,232, 0.8)","THEME_COLOR_LIMIT":12,"radius":5,"strokeWidth":1,"nodeBkg":"#ECECFF","nodeBorder":"#9370DB","clusterBkg":"#ffffde","clusterBorder":"#aaaa33","defaultLinkColor":"#333333","titleColor":"#333","edgeLabelBackground":"rgba(232,232,232, 0.8)","actorBorder":"#9370DB","actorBkg":"#ECECFF","actorTextColor":"black","actorLineColor":"#9370DB","signalColor":"#333","signalTextColor":"#333","labelBoxBkgColor":"#ECECFF","labelBoxBorderColor":"#9370DB","labelTextColor":"black","loopTextColor":"black","noteBorderColor":"#aaaa33","noteBkgColor":"#fff5ad","noteTextColor":"black","activationBorderColor":"#666","activationBkgColor":"#f4f4f4","sequenceNumberColor":"white","sectionBkgColor":"rgba(102, 102, 255, 0.49)","altSectionBkgColor":"white","sectionBkgColor2":"#fff400","excludeBkgColor":"#eeeeee","taskBorderColor":"#534fbc","taskBkgColor":"#8a90dd","taskTextLightColor":"white","taskTextColor":"white","taskTextDarkColor":"black","taskTextOutsideColor":"black","taskTextClickableColor":"#003163","activeTaskBorderColor":"#534fbc","activeTaskBkgColor":"#bfc7ff","gridColor":"lightgrey","doneTaskBkgColor":"lightgrey","doneTaskBorderColor":"grey","critBorderColor":"#ff8888","critBkgColor":"red","todayLineColor":"red","vertLineColor":"navy","noteFontWeight":"normal","fontWeight":"normal","personBorder":"hsl(240, 60%, 86.2745098039%)","personBkg":"#ECECFF","archEdgeColor":"#333333","archEdgeArrowColor":"#333333","archEdgeWidth":"3","archGroupBorderColor":"hsl(240, 60%, 86.2745098039%)","archGroupBorderWidth":"2px","rowOdd":"hsl(240, 100%, 100%)","rowEven":"hsl(240, 100%, 97.2745098039%)","labelColor":"black","errorBkgColor":"#552222","errorTextColor":"#552222","useGradient":false,"gradientStart":"hsl(240, 60%, 86.2745098039%)","gradientStop":"hsl(60, 60%, 83.5294117647%)","dropShadow":"drop-shadow(1px 2px 2px rgba(185, 185, 185, 1))","cScale0":"hsl(240, 100%, 76.2745098039%)","cScale1":"hsl(60, 100%, 73.5294117647%)","cScale2":"hsl(80, 100%, 76.2745098039%)","cScale3":"hsl(270, 100%, 76.2745098039%)","cScale4":"hsl(300, 100%, 76.2745098039%)","cScale5":"hsl(330, 100%, 76.2745098039%)","cScale6":"hsl(0, 100%, 76.2745098039%)","cScale7":"hsl(30, 100%, 76.2745098039%)","cScale8":"hsl(90, 100%, 76.2745098039%)","cScale9":"hsl(150, 100%, 76.2745098039%)","cScale10":"hsl(180, 100%, 76.2745098039%)","cScale11":"hsl(210, 100%, 76.2745098039%)","cScalePeer1":"hsl(60, 100%, 48.5294117647%)","cScalePeer2":"hsl(80, 100%, 56.2745098039%)","cScalePeer0":"hsl(240, 100%, 61.2745098039%)","cScalePeer3":"hsl(270, 100%, 61.2745098039%)","cScalePeer4":"hsl(300, 100%, 61.2745098039%)","cScalePeer5":"hsl(330, 100%, 61.2745098039%)","cScalePeer6":"hsl(0, 100%, 61.2745098039%)","cScalePeer7":"hsl(30, 100%, 61.2745098039%)","cScalePeer8":"hsl(90, 100%, 61.2745098039%)","cScalePeer9":"hsl(150, 100%, 61.2745098039%)","cScalePeer10":"hsl(180, 100%, 61.2745098039%)","cScalePeer11":"hsl(210, 100%, 61.2745098039%)","cScaleInv0":"hsl(60, 100%, 86.2745098039%)","cScaleInv1":"hsl(240, 100%, 83.5294117647%)","cScaleInv2":"hsl(260, 100%, 86.2745098039%)","cScaleInv3":"hsl(90, 100%, 86.2745098039%)","cScaleInv4":"hsl(120, 100%, 86.2745098039%)","cScaleInv5":"hsl(150, 100%, 86.2745098039%)","cScaleInv6":"hsl(180, 100%, 86.2745098039%)","cScaleInv7":"hsl(210, 100%, 86.2745098039%)","cScaleInv8":"hsl(270, 100%, 86.2745098039%)","cScaleInv9":"hsl(330, 100%, 86.2745098039%)","cScaleInv10":"hsl(0, 100%, 86.2745098039%)","cScaleInv11":"hsl(30, 100%, 86.2745098039%)","surface0":"hsl(270, 100%, 91.2745098039%)","surfacePeer0":"hsl(270, 100%, 89.2745098039%)","surface1":"hsl(270, 100%, 86.2745098039%)","surfacePeer1":"hsl(270, 100%, 84.2745098039%)","surface2":"hsl(270, 100%, 81.2745098039%)","surfacePeer2":"hsl(270, 100%, 79.2745098039%)","surface3":"hsl(270, 100%, 76.2745098039%)","surfacePeer3":"hsl(270, 100%, 74.2745098039%)","surface4":"hsl(270, 100%, 71.2745098039%)","surfacePeer4":"hsl(270, 100%, 69.2745098039%)","scaleLabelColor":"black","flowContainerStroke":"hsl(60, 60%, 83.5294117647%)","rectBkgColor":"hsl(80, 100%, 96.2745098039%)","transitionColor":"#333333","transitionLabelColor":"#333","stateLabelColor":"#131300","stateBkg":"#ECECFF","labelBackgroundColor":"#ECECFF","compositeBackground":"white","altBackground":"#f0f0f0","compositeTitleBackground":"#ECECFF","compositeBorder":"#9370DB","innerEndBackground":"#9370DB","specialStateColor":"#333333","classText":"#131300","fillType0":"#ECECFF","fillType1":"#ffffde","fillType2":"hsl(304, 100%, 96.2745098039%)","fillType3":"hsl(124, 100%, 93.5294117647%)","fillType4":"hsl(176, 100%, 96.2745098039%)","fillType5":"hsl(-4, 100%, 93.5294117647%)","fillType6":"hsl(8, 100%, 96.2745098039%)","fillType7":"hsl(188, 100%, 93.5294117647%)","pie1":"#ECECFF","pie2":"#ffffde","pie3":"hsl(80, 100%, 56.2745098039%)","pie4":"hsl(240, 100%, 86.2745098039%)","pie5":"hsl(60, 100%, 63.5294117647%)","pie6":"hsl(80, 100%, 76.2745098039%)","pie7":"hsl(300, 100%, 76.2745098039%)","pie8":"hsl(180, 100%, 56.2745098039%)","pie9":"hsl(0, 100%, 56.2745098039%)","pie10":"hsl(300, 100%, 56.2745098039%)","pie11":"hsl(150, 100%, 56.2745098039%)","pie12":"hsl(0, 100%, 66.2745098039%)","pieTitleTextSize":"25px","pieTitleTextColor":"black","pieSectionTextSize":"17px","pieSectionTextColor":"#333","pieLegendTextSize":"17px","pieLegendTextColor":"black","pieStrokeColor":"black","pieStrokeWidth":"2px","pieOuterStrokeWidth":"2px","pieOuterStrokeColor":"black","pieOpacity":"0.7","venn1":"hsl(240, 100%, 66.2745098039%)","venn2":"hsl(60, 100%, 63.5294117647%)","venn3":"hsl(80, 100%, 56.2745098039%)","venn4":"hsl(300, 100%, 66.2745098039%)","venn5":"hsl(180, 100%, 66.2745098039%)","venn6":"hsl(120, 100%, 63.5294117647%)","venn7":"hsl(0, 100%, 66.2745098039%)","venn8":"hsl(180, 100%, 63.5294117647%)","vennTitleTextColor":"#333","vennSetTextColor":"#333","cynefin":{"domainFontSize":16,"itemFontSize":12,"boundaryColor":"#333333","boundaryWidth":2,"cliffColor":"#8B0000","cliffWidth":4,"arrowColor":"#333333","arrowWidth":2,"complexBg":"#E8F5E9","complicatedBg":"#E3F2FD","chaoticBg":"#FBE9E7","clearBg":"#FFF8E1","confusionBg":"#F3E5F5","textColor":"#333","labelColor":"#131300"},"quadrant1Fill":"#ECECFF","quadrant2Fill":"#f1f1ff","quadrant3Fill":"#f6f6ff","quadrant4Fill":"#fbfbff","quadrant1TextFill":"#131300","quadrant2TextFill":"#0e0e00","quadrant3TextFill":"#090900","quadrant4TextFill":"#040400","quadrantPointFill":"hsl(240, 100%, NaN%)","quadrantPointTextFill":"#131300","quadrantXAxisTextFill":"#131300","quadrantYAxisTextFill":"#131300","quadrantInternalBorderStrokeFill":"hsl(240, 60%, 86.2745098039%)","quadrantExternalBorderStrokeFill":"hsl(240, 60%, 86.2745098039%)","quadrantTitleFill":"#131300","radar":{"axisColor":"#333333","axisStrokeWidth":2,"axisLabelFontSize":12,"curveOpacity":0.5,"curveStrokeWidth":2,"graticuleColor":"#DEDEDE","graticuleStrokeWidth":1,"graticuleOpacity":0.3,"legendBoxSize":12,"legendFontSize":12},"wardleyEvolutionColor":"#dc3545","wardley":{"backgroundColor":"white","axisColor":"#333333","axisTextColor":"#131300","gridColor":"lightgrey","componentFill":"white","componentStroke":"#333333","componentLabelColor":"#131300","linkStroke":"#333333","evolutionStroke":"#dc3545","annotationStroke":"#333333","annotationTextColor":"#131300","annotationFill":"white"},"xyChart":{"backgroundColor":"white","titleColor":"#131300","dataLabelColor":"#131300","legendTextColor":"#131300","xAxisTitleColor":"#131300","xAxisLabelColor":"#131300","xAxisTickColor":"#131300","xAxisLineColor":"#131300","yAxisTitleColor":"#131300","yAxisLabelColor":"#131300","yAxisTickColor":"#131300","yAxisLineColor":"#131300","plotColorPalette":"#ECECFF,#8493A6,#FFC3A0,#DCDDE1,#B8E994,#D1A36F,#C3CDE6,#FFB6C1,#496078,#F8F3E3"},"requirementBackground":"#ECECFF","requirementBorderColor":"hsl(240, 60%, 86.2745098039%)","requirementBorderSize":"1","requirementTextColor":"#131300","relationColor":"#333333","relationLabelBackground":"rgba(232,232,232, 0.8)","relationLabelColor":"black","git0":"hsl(240, 100%, 46.2745098039%)","git1":"hsl(60, 100%, 43.5294117647%)","git2":"hsl(80, 100%, 46.2745098039%)","git3":"hsl(210, 100%, 46.2745098039%)","git4":"hsl(180, 100%, 46.2745098039%)","git5":"hsl(150, 100%, 46.2745098039%)","git6":"hsl(300, 100%, 46.2745098039%)","git7":"hsl(0, 100%, 46.2745098039%)","gitInv0":"hsl(60, 100%, 3.7254901961%)","gitInv1":"rgb(0, 0, 160.5)","gitInv2":"rgb(48.8333333334, 0, 146.5000000001)","gitInv3":"rgb(146.5000000001, 73.2500000001, 0)","gitInv4":"rgb(146.5000000001, 0, 0)","gitInv5":"rgb(146.5000000001, 0, 73.2500000001)","gitInv6":"rgb(0, 146.5000000001, 0)","gitInv7":"rgb(0, 146.5000000001, 146.5000000001)","gitBranchLabel0":"#ffffff","gitBranchLabel1":"black","gitBranchLabel2":"black","gitBranchLabel3":"#ffffff","gitBranchLabel4":"black","gitBranchLabel5":"black","gitBranchLabel6":"black","gitBranchLabel7":"black","tagLabelColor":"#131300","tagLabelBackground":"#ECECFF","tagLabelBorder":"hsl(240, 60%, 86.2745098039%)","tagLabelFontSize":"10px","commitLabelColor":"#000021","commitLabelBackground":"#ffffde","commitLabelFontSize":"10px","emUiFill":"white","emUiStroke":"#dbdada","emProcessorFill":"#edb3f6","emProcessorStroke":"#b88cbf","emReadModelFill":"#d3f1a2","emReadModelStroke":"#a3b732","emCommandFill":"#bcd6fe","emCommandStroke":"#679ac3","emEventFill":"#ffb778","emEventStroke":"#c19a0f","emSwimlaneBackgroundOdd":"rgb(250,250,250)","emSwimlaneBackgroundStroke":"rgb(240,240,240)","emArrowhead":"#333333","emRelationStroke":"#333333","attributeBackgroundColorOdd":"#ffffff","attributeBackgroundColorEven":"#f2f2f2","cScaleLabel0":"#ffffff","cScaleLabel3":"#ffffff","cScaleLabel1":"black","cScaleLabel2":"black","cScaleLabel4":"black","cScaleLabel5":"black","cScaleLabel6":"black","cScaleLabel7":"black","cScaleLabel8":"black","cScaleLabel9":"black","cScaleLabel10":"black","cScaleLabel11":"black"}"##;
    const DEFAULT_COLORS: &str = r##"{"background":"white","primaryColor":"#abcdef","secondaryColor":"#123456","tertiaryColor":"hsl(80, 100%, 96.2745098039%)","primaryBorderColor":"hsl(240, 60%, 86.2745098039%)","secondaryBorderColor":"hsl(60, 60%, 83.5294117647%)","tertiaryBorderColor":"hsl(80, 60%, 86.2745098039%)","primaryTextColor":"#131300","secondaryTextColor":"#000021","tertiaryTextColor":"rgb(6.3333333334, 0, 19.0000000001)","lineColor":"#333333","textColor":"#333","mainBkg":"#ECECFF","secondBkg":"#ffffde","border1":"#9370DB","border2":"#aaaa33","arrowheadColor":"#333333","fontFamily":"\"trebuchet ms\", verdana, arial, sans-serif","fontSize":"16px","labelBackground":"rgba(232,232,232, 0.8)","THEME_COLOR_LIMIT":12,"radius":5,"strokeWidth":1,"nodeBkg":"#ECECFF","nodeBorder":"#9370DB","clusterBkg":"#ffffde","clusterBorder":"#aaaa33","defaultLinkColor":"#333333","titleColor":"#333","edgeLabelBackground":"rgba(232,232,232, 0.8)","actorBorder":"#9370DB","actorBkg":"#ECECFF","actorTextColor":"black","actorLineColor":"#9370DB","signalColor":"#333","signalTextColor":"#333","labelBoxBkgColor":"#ECECFF","labelBoxBorderColor":"#9370DB","labelTextColor":"black","loopTextColor":"black","noteBorderColor":"#aaaa33","noteBkgColor":"#fff5ad","noteTextColor":"black","activationBorderColor":"#666","activationBkgColor":"#f4f4f4","sequenceNumberColor":"white","sectionBkgColor":"rgba(102, 102, 255, 0.49)","altSectionBkgColor":"white","sectionBkgColor2":"#fff400","excludeBkgColor":"#eeeeee","taskBorderColor":"#534fbc","taskBkgColor":"#8a90dd","taskTextLightColor":"white","taskTextColor":"white","taskTextDarkColor":"black","taskTextOutsideColor":"black","taskTextClickableColor":"#003163","activeTaskBorderColor":"#534fbc","activeTaskBkgColor":"#bfc7ff","gridColor":"lightgrey","doneTaskBkgColor":"lightgrey","doneTaskBorderColor":"grey","critBorderColor":"#ff8888","critBkgColor":"red","todayLineColor":"red","vertLineColor":"navy","noteFontWeight":"normal","fontWeight":"normal","personBorder":"hsl(240, 60%, 86.2745098039%)","personBkg":"#ECECFF","archEdgeColor":"#333333","archEdgeArrowColor":"#333333","archEdgeWidth":"3","archGroupBorderColor":"hsl(240, 60%, 86.2745098039%)","archGroupBorderWidth":"2px","rowOdd":"hsl(210, 68%, 100%)","rowEven":"hsl(210, 68%, 81.3921568627%)","labelColor":"black","errorBkgColor":"#552222","errorTextColor":"#552222","useGradient":false,"gradientStart":"hsl(240, 60%, 86.2745098039%)","gradientStop":"hsl(60, 60%, 83.5294117647%)","dropShadow":"drop-shadow(1px 2px 2px rgba(185, 185, 185, 1))","cScale0":"hsl(240, 100%, 76.2745098039%)","cScale1":"hsl(60, 100%, 73.5294117647%)","cScale2":"hsl(80, 100%, 76.2745098039%)","cScale3":"hsl(270, 100%, 76.2745098039%)","cScale4":"hsl(300, 100%, 76.2745098039%)","cScale5":"hsl(330, 100%, 76.2745098039%)","cScale6":"hsl(0, 100%, 76.2745098039%)","cScale7":"hsl(30, 100%, 76.2745098039%)","cScale8":"hsl(90, 100%, 76.2745098039%)","cScale9":"hsl(150, 100%, 76.2745098039%)","cScale10":"hsl(180, 100%, 76.2745098039%)","cScale11":"hsl(210, 100%, 76.2745098039%)","cScalePeer1":"hsl(60, 100%, 48.5294117647%)","cScalePeer2":"hsl(80, 100%, 56.2745098039%)","cScalePeer0":"hsl(240, 100%, 61.2745098039%)","cScalePeer3":"hsl(270, 100%, 61.2745098039%)","cScalePeer4":"hsl(300, 100%, 61.2745098039%)","cScalePeer5":"hsl(330, 100%, 61.2745098039%)","cScalePeer6":"hsl(0, 100%, 61.2745098039%)","cScalePeer7":"hsl(30, 100%, 61.2745098039%)","cScalePeer8":"hsl(90, 100%, 61.2745098039%)","cScalePeer9":"hsl(150, 100%, 61.2745098039%)","cScalePeer10":"hsl(180, 100%, 61.2745098039%)","cScalePeer11":"hsl(210, 100%, 61.2745098039%)","cScaleInv0":"hsl(60, 100%, 86.2745098039%)","cScaleInv1":"hsl(240, 100%, 83.5294117647%)","cScaleInv2":"hsl(260, 100%, 86.2745098039%)","cScaleInv3":"hsl(90, 100%, 86.2745098039%)","cScaleInv4":"hsl(120, 100%, 86.2745098039%)","cScaleInv5":"hsl(150, 100%, 86.2745098039%)","cScaleInv6":"hsl(180, 100%, 86.2745098039%)","cScaleInv7":"hsl(210, 100%, 86.2745098039%)","cScaleInv8":"hsl(270, 100%, 86.2745098039%)","cScaleInv9":"hsl(330, 100%, 86.2745098039%)","cScaleInv10":"hsl(0, 100%, 86.2745098039%)","cScaleInv11":"hsl(30, 100%, 86.2745098039%)","surface0":"hsl(270, 100%, 91.2745098039%)","surfacePeer0":"hsl(270, 100%, 89.2745098039%)","surface1":"hsl(270, 100%, 86.2745098039%)","surfacePeer1":"hsl(270, 100%, 84.2745098039%)","surface2":"hsl(270, 100%, 81.2745098039%)","surfacePeer2":"hsl(270, 100%, 79.2745098039%)","surface3":"hsl(270, 100%, 76.2745098039%)","surfacePeer3":"hsl(270, 100%, 74.2745098039%)","surface4":"hsl(270, 100%, 71.2745098039%)","surfacePeer4":"hsl(270, 100%, 69.2745098039%)","scaleLabelColor":"black","flowContainerStroke":"hsl(60, 60%, 83.5294117647%)","rectBkgColor":"hsl(80, 100%, 96.2745098039%)","transitionColor":"#333333","transitionLabelColor":"#333","stateLabelColor":"#131300","stateBkg":"#ECECFF","labelBackgroundColor":"#ECECFF","compositeBackground":"white","altBackground":"#f0f0f0","compositeTitleBackground":"#ECECFF","compositeBorder":"#9370DB","innerEndBackground":"#9370DB","specialStateColor":"#333333","classText":"#131300","fillType0":"#abcdef","fillType1":"#123456","fillType2":"hsl(274, 68%, 80.3921568627%)","fillType3":"hsl(274, 65.3846153846%, 20.3921568627%)","fillType4":"hsl(146, 68%, 80.3921568627%)","fillType5":"hsl(146, 65.3846153846%, 20.3921568627%)","fillType6":"hsl(338, 68%, 80.3921568627%)","fillType7":"hsl(338, 65.3846153846%, 20.3921568627%)","pie1":"#ECECFF","pie2":"#ffffde","pie3":"hsl(80, 100%, 56.2745098039%)","pie4":"hsl(240, 100%, 86.2745098039%)","pie5":"hsl(60, 100%, 63.5294117647%)","pie6":"hsl(80, 100%, 76.2745098039%)","pie7":"hsl(300, 100%, 76.2745098039%)","pie8":"hsl(180, 100%, 56.2745098039%)","pie9":"hsl(0, 100%, 56.2745098039%)","pie10":"hsl(300, 100%, 56.2745098039%)","pie11":"hsl(150, 100%, 56.2745098039%)","pie12":"hsl(0, 100%, 66.2745098039%)","pieTitleTextSize":"25px","pieTitleTextColor":"black","pieSectionTextSize":"17px","pieSectionTextColor":"#333","pieLegendTextSize":"17px","pieLegendTextColor":"black","pieStrokeColor":"black","pieStrokeWidth":"2px","pieOuterStrokeWidth":"2px","pieOuterStrokeColor":"black","pieOpacity":"0.7","venn1":"hsl(240, 100%, 66.2745098039%)","venn2":"hsl(60, 100%, 63.5294117647%)","venn3":"hsl(80, 100%, 56.2745098039%)","venn4":"hsl(300, 100%, 66.2745098039%)","venn5":"hsl(180, 100%, 66.2745098039%)","venn6":"hsl(120, 100%, 63.5294117647%)","venn7":"hsl(0, 100%, 66.2745098039%)","venn8":"hsl(180, 100%, 63.5294117647%)","vennTitleTextColor":"#333","vennSetTextColor":"#333","cynefin":{"domainFontSize":16,"itemFontSize":12,"boundaryColor":"#333333","boundaryWidth":2,"cliffColor":"#8B0000","cliffWidth":4,"arrowColor":"#333333","arrowWidth":2,"complexBg":"#E8F5E9","complicatedBg":"#E3F2FD","chaoticBg":"#FBE9E7","clearBg":"#FFF8E1","confusionBg":"#F3E5F5","textColor":"#333","labelColor":"#131300"},"quadrant1Fill":"#ECECFF","quadrant2Fill":"#f1f1ff","quadrant3Fill":"#f6f6ff","quadrant4Fill":"#fbfbff","quadrant1TextFill":"#131300","quadrant2TextFill":"#0e0e00","quadrant3TextFill":"#090900","quadrant4TextFill":"#040400","quadrantPointFill":"hsl(240, 100%, NaN%)","quadrantPointTextFill":"#131300","quadrantXAxisTextFill":"#131300","quadrantYAxisTextFill":"#131300","quadrantInternalBorderStrokeFill":"hsl(240, 60%, 86.2745098039%)","quadrantExternalBorderStrokeFill":"hsl(240, 60%, 86.2745098039%)","quadrantTitleFill":"#131300","radar":{"axisColor":"#333333","axisStrokeWidth":2,"axisLabelFontSize":12,"curveOpacity":0.5,"curveStrokeWidth":2,"graticuleColor":"#DEDEDE","graticuleStrokeWidth":1,"graticuleOpacity":0.3,"legendBoxSize":12,"legendFontSize":12},"wardleyEvolutionColor":"#dc3545","wardley":{"backgroundColor":"white","axisColor":"#333333","axisTextColor":"#131300","gridColor":"lightgrey","componentFill":"white","componentStroke":"#333333","componentLabelColor":"#131300","linkStroke":"#333333","evolutionStroke":"#dc3545","annotationStroke":"#333333","annotationTextColor":"#131300","annotationFill":"white"},"xyChart":{"backgroundColor":"white","titleColor":"#131300","dataLabelColor":"#131300","legendTextColor":"#131300","xAxisTitleColor":"#131300","xAxisLabelColor":"#131300","xAxisTickColor":"#131300","xAxisLineColor":"#131300","yAxisTitleColor":"#131300","yAxisLabelColor":"#131300","yAxisTickColor":"#131300","yAxisLineColor":"#131300","plotColorPalette":"#ECECFF,#8493A6,#FFC3A0,#DCDDE1,#B8E994,#D1A36F,#C3CDE6,#FFB6C1,#496078,#F8F3E3"},"requirementBackground":"#ECECFF","requirementBorderColor":"hsl(240, 60%, 86.2745098039%)","requirementBorderSize":"1","requirementTextColor":"#131300","relationColor":"#333333","relationLabelBackground":"rgba(232,232,232, 0.8)","relationLabelColor":"black","git0":"hsl(240, 100%, 96.2745098039%)","git1":"hsl(60, 100%, 93.5294117647%)","git2":"hsl(80, 100%, 96.2745098039%)","git3":"hsl(210, 100%, 96.2745098039%)","git4":"hsl(180, 100%, 96.2745098039%)","git5":"hsl(150, 100%, 96.2745098039%)","git6":"hsl(300, 100%, 96.2745098039%)","git7":"hsl(0, 100%, 96.2745098039%)","gitInv0":"hsl(60, 100%, 3.7254901961%)","gitInv1":"rgb(0, 0, 160.5)","gitInv2":"rgb(48.8333333334, 0, 146.5000000001)","gitInv3":"rgb(146.5000000001, 73.2500000001, 0)","gitInv4":"rgb(146.5000000001, 0, 0)","gitInv5":"rgb(146.5000000001, 0, 73.2500000001)","gitInv6":"rgb(0, 146.5000000001, 0)","gitInv7":"rgb(0, 146.5000000001, 146.5000000001)","gitBranchLabel0":"#ffffff","gitBranchLabel1":"black","gitBranchLabel2":"black","gitBranchLabel3":"#ffffff","gitBranchLabel4":"black","gitBranchLabel5":"black","gitBranchLabel6":"black","gitBranchLabel7":"black","tagLabelColor":"#131300","tagLabelBackground":"#ECECFF","tagLabelBorder":"hsl(240, 60%, 86.2745098039%)","tagLabelFontSize":"10px","commitLabelColor":"#000021","commitLabelBackground":"#ffffde","commitLabelFontSize":"10px","emUiFill":"white","emUiStroke":"#dbdada","emProcessorFill":"#edb3f6","emProcessorStroke":"#b88cbf","emReadModelFill":"#d3f1a2","emReadModelStroke":"#a3b732","emCommandFill":"#bcd6fe","emCommandStroke":"#679ac3","emEventFill":"#ffb778","emEventStroke":"#c19a0f","emSwimlaneBackgroundOdd":"rgb(250,250,250)","emSwimlaneBackgroundStroke":"rgb(240,240,240)","emArrowhead":"#333333","emRelationStroke":"#333333","attributeBackgroundColorOdd":"#ffffff","attributeBackgroundColorEven":"#f2f2f2","darkMode":true,"cScaleLabel0":"#ffffff","cScaleLabel3":"#ffffff","cScaleLabel1":"black","cScaleLabel2":"black","cScaleLabel4":"black","cScaleLabel5":"black","cScaleLabel6":"black","cScaleLabel7":"black","cScaleLabel8":"black","cScaleLabel9":"black","cScaleLabel10":"black","cScaleLabel11":"black"}"##;
    const DEFAULT_NESTED: &str = r##"{"background":"white","primaryColor":"#ECECFF","secondaryColor":"#ffffde","tertiaryColor":"hsl(80, 100%, 96.2745098039%)","primaryBorderColor":"hsl(240, 60%, 86.2745098039%)","secondaryBorderColor":"hsl(60, 60%, 83.5294117647%)","tertiaryBorderColor":"hsl(80, 60%, 86.2745098039%)","primaryTextColor":"#131300","secondaryTextColor":"#000021","tertiaryTextColor":"rgb(6.3333333334, 0, 19.0000000001)","lineColor":"#333333","textColor":"#333","mainBkg":"#ECECFF","secondBkg":"#ffffde","border1":"#9370DB","border2":"#aaaa33","arrowheadColor":"#333333","fontFamily":"\"trebuchet ms\", verdana, arial, sans-serif","fontSize":"16px","labelBackground":"rgba(232,232,232, 0.8)","THEME_COLOR_LIMIT":12,"radius":5,"strokeWidth":1,"nodeBkg":"#ECECFF","nodeBorder":"#9370DB","clusterBkg":"#ffffde","clusterBorder":"#aaaa33","defaultLinkColor":"#333333","titleColor":"#333","edgeLabelBackground":"rgba(232,232,232, 0.8)","actorBorder":"#9370DB","actorBkg":"#ECECFF","actorTextColor":"black","actorLineColor":"#9370DB","signalColor":"#333","signalTextColor":"#333","labelBoxBkgColor":"#ECECFF","labelBoxBorderColor":"#9370DB","labelTextColor":"black","loopTextColor":"black","noteBorderColor":"#aaaa33","noteBkgColor":"#fff5ad","noteTextColor":"black","activationBorderColor":"#666","activationBkgColor":"#f4f4f4","sequenceNumberColor":"white","sectionBkgColor":"rgba(102, 102, 255, 0.49)","altSectionBkgColor":"white","sectionBkgColor2":"#fff400","excludeBkgColor":"#eeeeee","taskBorderColor":"#534fbc","taskBkgColor":"#8a90dd","taskTextLightColor":"white","taskTextColor":"white","taskTextDarkColor":"black","taskTextOutsideColor":"black","taskTextClickableColor":"#003163","activeTaskBorderColor":"#534fbc","activeTaskBkgColor":"#bfc7ff","gridColor":"lightgrey","doneTaskBkgColor":"lightgrey","doneTaskBorderColor":"grey","critBorderColor":"#ff8888","critBkgColor":"red","todayLineColor":"red","vertLineColor":"navy","noteFontWeight":"normal","fontWeight":"normal","personBorder":"hsl(240, 60%, 86.2745098039%)","personBkg":"#ECECFF","archEdgeColor":"#333333","archEdgeArrowColor":"#333333","archEdgeWidth":"3","archGroupBorderColor":"hsl(240, 60%, 86.2745098039%)","archGroupBorderWidth":"2px","rowOdd":"hsl(240, 100%, 100%)","rowEven":"hsl(240, 100%, 97.2745098039%)","labelColor":"black","errorBkgColor":"#552222","errorTextColor":"#552222","useGradient":false,"gradientStart":"hsl(240, 60%, 86.2745098039%)","gradientStop":"hsl(60, 60%, 83.5294117647%)","dropShadow":"drop-shadow(1px 2px 2px rgba(185, 185, 185, 1))","cScale0":"hsl(240, 100%, 76.2745098039%)","cScale1":"hsl(60, 100%, 73.5294117647%)","cScale2":"hsl(80, 100%, 76.2745098039%)","cScale3":"hsl(270, 100%, 76.2745098039%)","cScale4":"hsl(300, 100%, 76.2745098039%)","cScale5":"hsl(330, 100%, 76.2745098039%)","cScale6":"hsl(0, 100%, 76.2745098039%)","cScale7":"hsl(30, 100%, 76.2745098039%)","cScale8":"hsl(90, 100%, 76.2745098039%)","cScale9":"hsl(150, 100%, 76.2745098039%)","cScale10":"hsl(180, 100%, 76.2745098039%)","cScale11":"hsl(210, 100%, 76.2745098039%)","cScalePeer1":"hsl(60, 100%, 48.5294117647%)","cScalePeer2":"hsl(80, 100%, 56.2745098039%)","cScalePeer0":"hsl(240, 100%, 61.2745098039%)","cScalePeer3":"hsl(270, 100%, 61.2745098039%)","cScalePeer4":"hsl(300, 100%, 61.2745098039%)","cScalePeer5":"hsl(330, 100%, 61.2745098039%)","cScalePeer6":"hsl(0, 100%, 61.2745098039%)","cScalePeer7":"hsl(30, 100%, 61.2745098039%)","cScalePeer8":"hsl(90, 100%, 61.2745098039%)","cScalePeer9":"hsl(150, 100%, 61.2745098039%)","cScalePeer10":"hsl(180, 100%, 61.2745098039%)","cScalePeer11":"hsl(210, 100%, 61.2745098039%)","cScaleInv0":"hsl(60, 100%, 86.2745098039%)","cScaleInv1":"hsl(240, 100%, 83.5294117647%)","cScaleInv2":"hsl(260, 100%, 86.2745098039%)","cScaleInv3":"hsl(90, 100%, 86.2745098039%)","cScaleInv4":"hsl(120, 100%, 86.2745098039%)","cScaleInv5":"hsl(150, 100%, 86.2745098039%)","cScaleInv6":"hsl(180, 100%, 86.2745098039%)","cScaleInv7":"hsl(210, 100%, 86.2745098039%)","cScaleInv8":"hsl(270, 100%, 86.2745098039%)","cScaleInv9":"hsl(330, 100%, 86.2745098039%)","cScaleInv10":"hsl(0, 100%, 86.2745098039%)","cScaleInv11":"hsl(30, 100%, 86.2745098039%)","surface0":"hsl(270, 100%, 91.2745098039%)","surfacePeer0":"hsl(270, 100%, 89.2745098039%)","surface1":"hsl(270, 100%, 86.2745098039%)","surfacePeer1":"hsl(270, 100%, 84.2745098039%)","surface2":"hsl(270, 100%, 81.2745098039%)","surfacePeer2":"hsl(270, 100%, 79.2745098039%)","surface3":"hsl(270, 100%, 76.2745098039%)","surfacePeer3":"hsl(270, 100%, 74.2745098039%)","surface4":"hsl(270, 100%, 71.2745098039%)","surfacePeer4":"hsl(270, 100%, 69.2745098039%)","scaleLabelColor":"black","flowContainerStroke":"hsl(60, 60%, 83.5294117647%)","rectBkgColor":"hsl(80, 100%, 96.2745098039%)","transitionColor":"#333333","transitionLabelColor":"#333","stateLabelColor":"#131300","stateBkg":"#ECECFF","labelBackgroundColor":"#ECECFF","compositeBackground":"white","altBackground":"#f0f0f0","compositeTitleBackground":"#ECECFF","compositeBorder":"#9370DB","innerEndBackground":"#9370DB","specialStateColor":"#333333","classText":"#131300","fillType0":"#ECECFF","fillType1":"#ffffde","fillType2":"hsl(304, 100%, 96.2745098039%)","fillType3":"hsl(124, 100%, 93.5294117647%)","fillType4":"hsl(176, 100%, 96.2745098039%)","fillType5":"hsl(-4, 100%, 93.5294117647%)","fillType6":"hsl(8, 100%, 96.2745098039%)","fillType7":"hsl(188, 100%, 93.5294117647%)","pie1":"#ECECFF","pie2":"#ffffde","pie3":"hsl(80, 100%, 56.2745098039%)","pie4":"hsl(240, 100%, 86.2745098039%)","pie5":"hsl(60, 100%, 63.5294117647%)","pie6":"hsl(80, 100%, 76.2745098039%)","pie7":"hsl(300, 100%, 76.2745098039%)","pie8":"hsl(180, 100%, 56.2745098039%)","pie9":"hsl(0, 100%, 56.2745098039%)","pie10":"hsl(300, 100%, 56.2745098039%)","pie11":"hsl(150, 100%, 56.2745098039%)","pie12":"hsl(0, 100%, 66.2745098039%)","pieTitleTextSize":"25px","pieTitleTextColor":"black","pieSectionTextSize":"17px","pieSectionTextColor":"#333","pieLegendTextSize":"17px","pieLegendTextColor":"black","pieStrokeColor":"black","pieStrokeWidth":"2px","pieOuterStrokeWidth":"2px","pieOuterStrokeColor":"black","pieOpacity":"0.7","venn1":"hsl(240, 100%, 66.2745098039%)","venn2":"hsl(60, 100%, 63.5294117647%)","venn3":"hsl(80, 100%, 56.2745098039%)","venn4":"hsl(300, 100%, 66.2745098039%)","venn5":"hsl(180, 100%, 66.2745098039%)","venn6":"hsl(120, 100%, 63.5294117647%)","venn7":"hsl(0, 100%, 66.2745098039%)","venn8":"hsl(180, 100%, 63.5294117647%)","vennTitleTextColor":"#333","vennSetTextColor":"#333","cynefin":{"domainFontSize":16,"itemFontSize":12,"boundaryColor":"#333333","boundaryWidth":2,"cliffColor":"#8B0000","cliffWidth":4,"arrowColor":"#333333","arrowWidth":2,"complexBg":"#E8F5E9","complicatedBg":"#E3F2FD","chaoticBg":"#FBE9E7","clearBg":"#FFF8E1","confusionBg":"#F3E5F5","textColor":"#333","labelColor":"#131300"},"quadrant1Fill":"#ECECFF","quadrant2Fill":"#f1f1ff","quadrant3Fill":"#f6f6ff","quadrant4Fill":"#fbfbff","quadrant1TextFill":"#131300","quadrant2TextFill":"#0e0e00","quadrant3TextFill":"#090900","quadrant4TextFill":"#040400","quadrantPointFill":"hsl(240, 100%, NaN%)","quadrantPointTextFill":"#131300","quadrantXAxisTextFill":"#131300","quadrantYAxisTextFill":"#131300","quadrantInternalBorderStrokeFill":"hsl(240, 60%, 86.2745098039%)","quadrantExternalBorderStrokeFill":"hsl(240, 60%, 86.2745098039%)","quadrantTitleFill":"#131300","radar":{"axisColor":"#333333","axisStrokeWidth":2,"axisLabelFontSize":12,"curveOpacity":0.5,"curveStrokeWidth":2,"graticuleColor":"#DEDEDE","graticuleStrokeWidth":1,"graticuleOpacity":0.3,"legendBoxSize":12,"legendFontSize":12},"wardleyEvolutionColor":"#dc3545","wardley":{"backgroundColor":"white","axisColor":"#333333","axisTextColor":"#131300","gridColor":"lightgrey","componentFill":"white","componentStroke":"#333333","componentLabelColor":"#131300","linkStroke":"#333333","evolutionStroke":"#dc3545","annotationStroke":"#333333","annotationTextColor":"#131300","annotationFill":"white"},"xyChart":{"backgroundColor":"white","titleColor":"#ff0000","dataLabelColor":"#131300","legendTextColor":"#131300","xAxisTitleColor":"#131300","xAxisLabelColor":"#131300","xAxisTickColor":"#131300","xAxisLineColor":"#131300","yAxisTitleColor":"#131300","yAxisLabelColor":"#131300","yAxisTickColor":"#131300","yAxisLineColor":"#131300","plotColorPalette":"#ECECFF,#8493A6,#FFC3A0,#DCDDE1,#B8E994,#D1A36F,#C3CDE6,#FFB6C1,#496078,#F8F3E3"},"requirementBackground":"#ECECFF","requirementBorderColor":"hsl(240, 60%, 86.2745098039%)","requirementBorderSize":"1","requirementTextColor":"#131300","relationColor":"#333333","relationLabelBackground":"rgba(232,232,232, 0.8)","relationLabelColor":"black","git0":"hsl(240, 100%, 46.2745098039%)","git1":"hsl(60, 100%, 43.5294117647%)","git2":"hsl(80, 100%, 46.2745098039%)","git3":"hsl(210, 100%, 46.2745098039%)","git4":"hsl(180, 100%, 46.2745098039%)","git5":"hsl(150, 100%, 46.2745098039%)","git6":"hsl(300, 100%, 46.2745098039%)","git7":"hsl(0, 100%, 46.2745098039%)","gitInv0":"hsl(60, 100%, 3.7254901961%)","gitInv1":"rgb(0, 0, 160.5)","gitInv2":"rgb(48.8333333334, 0, 146.5000000001)","gitInv3":"rgb(146.5000000001, 73.2500000001, 0)","gitInv4":"rgb(146.5000000001, 0, 0)","gitInv5":"rgb(146.5000000001, 0, 73.2500000001)","gitInv6":"rgb(0, 146.5000000001, 0)","gitInv7":"rgb(0, 146.5000000001, 146.5000000001)","gitBranchLabel0":"#ffffff","gitBranchLabel1":"black","gitBranchLabel2":"black","gitBranchLabel3":"#ffffff","gitBranchLabel4":"black","gitBranchLabel5":"black","gitBranchLabel6":"black","gitBranchLabel7":"black","tagLabelColor":"#131300","tagLabelBackground":"#ECECFF","tagLabelBorder":"hsl(240, 60%, 86.2745098039%)","tagLabelFontSize":"10px","commitLabelColor":"#000021","commitLabelBackground":"#ffffde","commitLabelFontSize":"10px","emUiFill":"white","emUiStroke":"#dbdada","emProcessorFill":"#edb3f6","emProcessorStroke":"#b88cbf","emReadModelFill":"#d3f1a2","emReadModelStroke":"#a3b732","emCommandFill":"#bcd6fe","emCommandStroke":"#679ac3","emEventFill":"#ffb778","emEventStroke":"#c19a0f","emSwimlaneBackgroundOdd":"rgb(250,250,250)","emSwimlaneBackgroundStroke":"rgb(240,240,240)","emArrowhead":"#333333","emRelationStroke":"#333333","attributeBackgroundColorOdd":"#ffffff","attributeBackgroundColorEven":"#f2f2f2","cScaleLabel0":"#ffffff","cScaleLabel3":"#ffffff","cScaleLabel1":"black","cScaleLabel2":"black","cScaleLabel4":"black","cScaleLabel5":"black","cScaleLabel6":"black","cScaleLabel7":"black","cScaleLabel8":"black","cScaleLabel9":"black","cScaleLabel10":"black","cScaleLabel11":"black"}"##;
    const DEFAULT_BORDER: &str = r##"{"background":"white","primaryColor":"#ECECFF","secondaryColor":"#ffffde","tertiaryColor":"hsl(80, 100%, 96.2745098039%)","primaryBorderColor":"hsl(240, 60%, 86.2745098039%)","secondaryBorderColor":"hsl(60, 60%, 83.5294117647%)","tertiaryBorderColor":"hsl(80, 60%, 86.2745098039%)","primaryTextColor":"#131300","secondaryTextColor":"#000021","tertiaryTextColor":"rgb(6.3333333334, 0, 19.0000000001)","lineColor":"#333333","textColor":"#333","mainBkg":"#ECECFF","secondBkg":"#ffffde","border1":"#9370DB","border2":"#aaaa33","arrowheadColor":"#333333","fontFamily":"Arial","fontSize":"16px","labelBackground":"rgba(232,232,232, 0.8)","THEME_COLOR_LIMIT":12,"radius":5,"strokeWidth":1,"nodeBkg":"#ECECFF","nodeBorder":"#225577","clusterBkg":"#ffffde","clusterBorder":"#aaaa33","defaultLinkColor":"#333333","titleColor":"#333","edgeLabelBackground":"rgba(232,232,232, 0.8)","actorBorder":"#9370DB","actorBkg":"#ECECFF","actorTextColor":"black","actorLineColor":"#9370DB","signalColor":"#333","signalTextColor":"#333","labelBoxBkgColor":"#ECECFF","labelBoxBorderColor":"#9370DB","labelTextColor":"black","loopTextColor":"black","noteBorderColor":"#aaaa33","noteBkgColor":"#fff5ad","noteTextColor":"black","activationBorderColor":"#666","activationBkgColor":"#f4f4f4","sequenceNumberColor":"white","sectionBkgColor":"rgba(102, 102, 255, 0.49)","altSectionBkgColor":"white","sectionBkgColor2":"#fff400","excludeBkgColor":"#eeeeee","taskBorderColor":"#534fbc","taskBkgColor":"#8a90dd","taskTextLightColor":"white","taskTextColor":"white","taskTextDarkColor":"black","taskTextOutsideColor":"black","taskTextClickableColor":"#003163","activeTaskBorderColor":"#534fbc","activeTaskBkgColor":"#bfc7ff","gridColor":"lightgrey","doneTaskBkgColor":"lightgrey","doneTaskBorderColor":"grey","critBorderColor":"#ff8888","critBkgColor":"red","todayLineColor":"red","vertLineColor":"navy","noteFontWeight":"normal","fontWeight":"normal","personBorder":"hsl(240, 60%, 86.2745098039%)","personBkg":"#ECECFF","archEdgeColor":"#333333","archEdgeArrowColor":"#333333","archEdgeWidth":"3","archGroupBorderColor":"hsl(240, 60%, 86.2745098039%)","archGroupBorderWidth":"2px","rowOdd":"hsl(240, 100%, 100%)","rowEven":"hsl(240, 100%, 97.2745098039%)","labelColor":"black","errorBkgColor":"#552222","errorTextColor":"#552222","useGradient":false,"gradientStart":"hsl(240, 60%, 86.2745098039%)","gradientStop":"hsl(60, 60%, 83.5294117647%)","dropShadow":"drop-shadow(1px 2px 2px rgba(185, 185, 185, 1))","cScale0":"#123456","cScale1":"hsl(60, 100%, 73.5294117647%)","cScale2":"hsl(80, 100%, 76.2745098039%)","cScale3":"hsl(270, 100%, 76.2745098039%)","cScale4":"hsl(300, 100%, 76.2745098039%)","cScale5":"hsl(330, 100%, 76.2745098039%)","cScale6":"hsl(0, 100%, 76.2745098039%)","cScale7":"hsl(30, 100%, 76.2745098039%)","cScale8":"hsl(90, 100%, 76.2745098039%)","cScale9":"hsl(150, 100%, 76.2745098039%)","cScale10":"hsl(180, 100%, 76.2745098039%)","cScale11":"hsl(210, 100%, 76.2745098039%)","cScalePeer1":"hsl(60, 100%, 48.5294117647%)","cScalePeer2":"hsl(80, 100%, 56.2745098039%)","cScalePeer0":"hsl(240, 100%, 61.2745098039%)","cScalePeer3":"hsl(270, 100%, 61.2745098039%)","cScalePeer4":"hsl(300, 100%, 61.2745098039%)","cScalePeer5":"hsl(330, 100%, 61.2745098039%)","cScalePeer6":"hsl(0, 100%, 61.2745098039%)","cScalePeer7":"hsl(30, 100%, 61.2745098039%)","cScalePeer8":"hsl(90, 100%, 61.2745098039%)","cScalePeer9":"hsl(150, 100%, 61.2745098039%)","cScalePeer10":"hsl(180, 100%, 61.2745098039%)","cScalePeer11":"hsl(210, 100%, 61.2745098039%)","cScaleInv0":"hsl(60, 100%, 86.2745098039%)","cScaleInv1":"hsl(240, 100%, 83.5294117647%)","cScaleInv2":"hsl(260, 100%, 86.2745098039%)","cScaleInv3":"hsl(90, 100%, 86.2745098039%)","cScaleInv4":"hsl(120, 100%, 86.2745098039%)","cScaleInv5":"hsl(150, 100%, 86.2745098039%)","cScaleInv6":"hsl(180, 100%, 86.2745098039%)","cScaleInv7":"hsl(210, 100%, 86.2745098039%)","cScaleInv8":"hsl(270, 100%, 86.2745098039%)","cScaleInv9":"hsl(330, 100%, 86.2745098039%)","cScaleInv10":"hsl(0, 100%, 86.2745098039%)","cScaleInv11":"hsl(30, 100%, 86.2745098039%)","surface0":"hsl(270, 100%, 91.2745098039%)","surfacePeer0":"hsl(270, 100%, 89.2745098039%)","surface1":"hsl(270, 100%, 86.2745098039%)","surfacePeer1":"hsl(270, 100%, 84.2745098039%)","surface2":"hsl(270, 100%, 81.2745098039%)","surfacePeer2":"hsl(270, 100%, 79.2745098039%)","surface3":"hsl(270, 100%, 76.2745098039%)","surfacePeer3":"hsl(270, 100%, 74.2745098039%)","surface4":"hsl(270, 100%, 71.2745098039%)","surfacePeer4":"hsl(270, 100%, 69.2745098039%)","scaleLabelColor":"black","flowContainerStroke":"hsl(60, 60%, 83.5294117647%)","rectBkgColor":"hsl(80, 100%, 96.2745098039%)","transitionColor":"#333333","transitionLabelColor":"#333","stateLabelColor":"#131300","stateBkg":"#ECECFF","labelBackgroundColor":"#ECECFF","compositeBackground":"white","altBackground":"#f0f0f0","compositeTitleBackground":"#ECECFF","compositeBorder":"#9370DB","innerEndBackground":"#9370DB","specialStateColor":"#333333","classText":"#131300","fillType0":"#ECECFF","fillType1":"#ffffde","fillType2":"hsl(304, 100%, 96.2745098039%)","fillType3":"hsl(124, 100%, 93.5294117647%)","fillType4":"hsl(176, 100%, 96.2745098039%)","fillType5":"hsl(-4, 100%, 93.5294117647%)","fillType6":"hsl(8, 100%, 96.2745098039%)","fillType7":"hsl(188, 100%, 93.5294117647%)","pie1":"#ECECFF","pie2":"#ffffde","pie3":"hsl(80, 100%, 56.2745098039%)","pie4":"hsl(240, 100%, 86.2745098039%)","pie5":"hsl(60, 100%, 63.5294117647%)","pie6":"hsl(80, 100%, 76.2745098039%)","pie7":"hsl(300, 100%, 76.2745098039%)","pie8":"hsl(180, 100%, 56.2745098039%)","pie9":"hsl(0, 100%, 56.2745098039%)","pie10":"hsl(300, 100%, 56.2745098039%)","pie11":"hsl(150, 100%, 56.2745098039%)","pie12":"hsl(0, 100%, 66.2745098039%)","pieTitleTextSize":"25px","pieTitleTextColor":"black","pieSectionTextSize":"17px","pieSectionTextColor":"#333","pieLegendTextSize":"17px","pieLegendTextColor":"black","pieStrokeColor":"black","pieStrokeWidth":"2px","pieOuterStrokeWidth":"2px","pieOuterStrokeColor":"black","pieOpacity":"0.7","venn1":"hsl(240, 100%, 66.2745098039%)","venn2":"hsl(60, 100%, 63.5294117647%)","venn3":"hsl(80, 100%, 56.2745098039%)","venn4":"hsl(300, 100%, 66.2745098039%)","venn5":"hsl(180, 100%, 66.2745098039%)","venn6":"hsl(120, 100%, 63.5294117647%)","venn7":"hsl(0, 100%, 66.2745098039%)","venn8":"hsl(180, 100%, 63.5294117647%)","vennTitleTextColor":"#333","vennSetTextColor":"#333","cynefin":{"domainFontSize":16,"itemFontSize":12,"boundaryColor":"#333333","boundaryWidth":2,"cliffColor":"#8B0000","cliffWidth":4,"arrowColor":"#333333","arrowWidth":2,"complexBg":"#E8F5E9","complicatedBg":"#E3F2FD","chaoticBg":"#FBE9E7","clearBg":"#FFF8E1","confusionBg":"#F3E5F5","textColor":"#333","labelColor":"#131300"},"quadrant1Fill":"#ECECFF","quadrant2Fill":"#f1f1ff","quadrant3Fill":"#f6f6ff","quadrant4Fill":"#fbfbff","quadrant1TextFill":"#131300","quadrant2TextFill":"#0e0e00","quadrant3TextFill":"#090900","quadrant4TextFill":"#040400","quadrantPointFill":"hsl(240, 100%, NaN%)","quadrantPointTextFill":"#131300","quadrantXAxisTextFill":"#131300","quadrantYAxisTextFill":"#131300","quadrantInternalBorderStrokeFill":"hsl(240, 60%, 86.2745098039%)","quadrantExternalBorderStrokeFill":"hsl(240, 60%, 86.2745098039%)","quadrantTitleFill":"#131300","radar":{"axisColor":"#333333","axisStrokeWidth":2,"axisLabelFontSize":12,"curveOpacity":0.5,"curveStrokeWidth":2,"graticuleColor":"#DEDEDE","graticuleStrokeWidth":1,"graticuleOpacity":0.3,"legendBoxSize":12,"legendFontSize":12},"wardleyEvolutionColor":"#dc3545","wardley":{"backgroundColor":"white","axisColor":"#333333","axisTextColor":"#131300","gridColor":"lightgrey","componentFill":"white","componentStroke":"#333333","componentLabelColor":"#131300","linkStroke":"#333333","evolutionStroke":"#dc3545","annotationStroke":"#333333","annotationTextColor":"#131300","annotationFill":"white"},"xyChart":{"backgroundColor":"white","titleColor":"#131300","dataLabelColor":"#131300","legendTextColor":"#131300","xAxisTitleColor":"#131300","xAxisLabelColor":"#131300","xAxisTickColor":"#131300","xAxisLineColor":"#131300","yAxisTitleColor":"#131300","yAxisLabelColor":"#131300","yAxisTickColor":"#131300","yAxisLineColor":"#131300","plotColorPalette":"#ECECFF,#8493A6,#FFC3A0,#DCDDE1,#B8E994,#D1A36F,#C3CDE6,#FFB6C1,#496078,#F8F3E3"},"requirementBackground":"#ECECFF","requirementBorderColor":"hsl(240, 60%, 86.2745098039%)","requirementBorderSize":"1","requirementTextColor":"#131300","relationColor":"#333333","relationLabelBackground":"rgba(232,232,232, 0.8)","relationLabelColor":"black","git0":"hsl(240, 100%, 46.2745098039%)","git1":"hsl(60, 100%, 43.5294117647%)","git2":"hsl(80, 100%, 46.2745098039%)","git3":"hsl(210, 100%, 46.2745098039%)","git4":"hsl(180, 100%, 46.2745098039%)","git5":"hsl(150, 100%, 46.2745098039%)","git6":"hsl(300, 100%, 46.2745098039%)","git7":"hsl(0, 100%, 46.2745098039%)","gitInv0":"hsl(60, 100%, 3.7254901961%)","gitInv1":"rgb(0, 0, 160.5)","gitInv2":"rgb(48.8333333334, 0, 146.5000000001)","gitInv3":"rgb(146.5000000001, 73.2500000001, 0)","gitInv4":"rgb(146.5000000001, 0, 0)","gitInv5":"rgb(146.5000000001, 0, 73.2500000001)","gitInv6":"rgb(0, 146.5000000001, 0)","gitInv7":"rgb(0, 146.5000000001, 146.5000000001)","gitBranchLabel0":"#ffffff","gitBranchLabel1":"black","gitBranchLabel2":"black","gitBranchLabel3":"#ffffff","gitBranchLabel4":"black","gitBranchLabel5":"black","gitBranchLabel6":"black","gitBranchLabel7":"black","tagLabelColor":"#131300","tagLabelBackground":"#ECECFF","tagLabelBorder":"hsl(240, 60%, 86.2745098039%)","tagLabelFontSize":"10px","commitLabelColor":"#000021","commitLabelBackground":"#ffffde","commitLabelFontSize":"10px","emUiFill":"white","emUiStroke":"#dbdada","emProcessorFill":"#edb3f6","emProcessorStroke":"#b88cbf","emReadModelFill":"#d3f1a2","emReadModelStroke":"#a3b732","emCommandFill":"#bcd6fe","emCommandStroke":"#679ac3","emEventFill":"#ffb778","emEventStroke":"#c19a0f","emSwimlaneBackgroundOdd":"rgb(250,250,250)","emSwimlaneBackgroundStroke":"rgb(240,240,240)","emArrowhead":"#333333","emRelationStroke":"#333333","attributeBackgroundColorOdd":"#ffffff","attributeBackgroundColorEven":"#f2f2f2","cScaleLabel0":"#ffffff","cScaleLabel3":"#ffffff","cScaleLabel1":"black","cScaleLabel2":"black","cScaleLabel4":"black","cScaleLabel5":"black","cScaleLabel6":"black","cScaleLabel7":"black","cScaleLabel8":"black","cScaleLabel9":"black","cScaleLabel10":"black","cScaleLabel11":"black"}"##;
    const DARK_NONE: &str = r##"{"background":"#333","primaryColor":"#1f2020","secondaryColor":"hsl(180, 1.5873015873%, 28.3529411765%)","tertiaryColor":"hsl(20, 1.5873015873%, 12.3529411765%)","primaryBorderColor":"#cccccc","secondaryBorderColor":"hsl(180, 0%, 18.3529411765%)","tertiaryBorderColor":"hsl(20, 0%, 2.3529411765%)","primaryTextColor":"#e0dfdf","secondaryTextColor":"rgb(183.8476190475, 181.5523809523, 181.5523809523)","tertiaryTextColor":"rgb(222.9999999999, 223.6666666666, 223.9999999999)","lineColor":"lightgrey","textColor":"#ccc","mainBkg":"#1f2020","secondBkg":"hsl(180, 1.5873015873%, 28.3529411765%)","mainContrastColor":"lightgrey","darkTextColor":"hsl(28.5714285714, 17.3553719008%, 86.2745098039%)","border1":"#ccc","border2":"rgba(255, 255, 255, 0.25)","arrowheadColor":"lightgrey","fontFamily":"\"trebuchet ms\", verdana, arial, sans-serif","fontSize":"16px","labelBackground":"#181818","THEME_COLOR_LIMIT":12,"radius":5,"strokeWidth":1,"nodeBkg":"#1f2020","nodeBorder":"#ccc","clusterBkg":"hsl(180, 1.5873015873%, 28.3529411765%)","clusterBorder":"rgba(255, 255, 255, 0.25)","defaultLinkColor":"lightgrey","titleColor":"#F9FFFE","edgeLabelBackground":"hsl(0, 0%, 34.4117647059%)","actorBorder":"#ccc","actorBkg":"#1f2020","actorTextColor":"lightgrey","actorLineColor":"#ccc","signalColor":"lightgrey","signalTextColor":"lightgrey","labelBoxBkgColor":"#1f2020","labelBoxBorderColor":"#ccc","labelTextColor":"lightgrey","loopTextColor":"lightgrey","noteBorderColor":"hsl(180, 0%, 18.3529411765%)","noteBkgColor":"hsl(180, 1.5873015873%, 28.3529411765%)","noteTextColor":"rgb(183.8476190475, 181.5523809523, 181.5523809523)","activationBorderColor":"#ccc","activationBkgColor":"hsl(180, 1.5873015873%, 28.3529411765%)","sequenceNumberColor":"black","sectionBkgColor":"hsl(52.9411764706, 28.813559322%, 58.431372549%)","altSectionBkgColor":"#333","sectionBkgColor2":"#EAE8D9","excludeBkgColor":"hsl(52.9411764706, 28.813559322%, 48.431372549%)","taskBorderColor":"#ffffff","taskBkgColor":"hsl(180, 1.5873015873%, 35.3529411765%)","taskTextColor":"hsl(28.5714285714, 17.3553719008%, 86.2745098039%)","taskTextLightColor":"lightgrey","taskTextOutsideColor":"lightgrey","taskTextClickableColor":"#003163","activeTaskBorderColor":"#ffffff","activeTaskBkgColor":"#81B1DB","gridColor":"lightgrey","doneTaskBkgColor":"lightgrey","doneTaskBorderColor":"grey","critBorderColor":"#E83737","critBkgColor":"#E83737","taskTextDarkColor":"#2c2c2c","todayLineColor":"#DB5757","vertLineColor":"#00BFFF","personBorder":"#cccccc","personBkg":"#1f2020","archEdgeColor":"lightgrey","archEdgeArrowColor":"lightgrey","archEdgeWidth":"3","archGroupBorderColor":"#cccccc","archGroupBorderWidth":"2px","rowOdd":"hsl(180, 1.5873015873%, 17.3529411765%)","rowEven":"hsl(180, 1.5873015873%, 2.3529411765%)","labelColor":"calculated","errorBkgColor":"#a44141","errorTextColor":"#ddd","useGradient":true,"gradientStart":"#cccccc","gradientStop":"hsl(180, 0%, 18.3529411765%)","dropShadow":"drop-shadow( 1px 2px 2px rgba(185,185,185,1))","noteFontWeight":"normal","fontWeight":"normal","flowContainerStroke":"hsl(180, 0%, 18.3529411765%)","rectBkgColor":"hsl(20, 1.5873015873%, 12.3529411765%)","transitionColor":"lightgrey","transitionLabelColor":"#ccc","stateLabelColor":"#e0dfdf","stateBkg":"#1f2020","labelBackgroundColor":"#1f2020","compositeBackground":"#333","altBackground":"#555","compositeTitleBackground":"#1f2020","compositeBorder":"#ccc","innerEndBackground":"#cccccc","specialStateColor":"#f4f4f4","fillType0":"#1f2020","fillType1":"hsl(180, 1.5873015873%, 28.3529411765%)","fillType2":"hsl(244, 1.5873015873%, 12.3529411765%)","fillType3":"hsl(244, 1.5873015873%, 28.3529411765%)","fillType4":"hsl(116, 1.5873015873%, 12.3529411765%)","fillType5":"hsl(116, 1.5873015873%, 28.3529411765%)","fillType6":"hsl(308, 1.5873015873%, 12.3529411765%)","fillType7":"hsl(308, 1.5873015873%, 28.3529411765%)","cScale1":"#0b0000","cScale2":"#4d1037","cScale3":"#3f5258","cScale4":"#4f2f1b","cScale5":"#6e0a0a","cScale6":"#3b0048","cScale7":"#995a01","cScale8":"#154706","cScale9":"#161722","cScale10":"#00296f","cScale11":"#01629c","cScale12":"#010029","cScale0":"#1f2020","cScaleInv0":"#e0dfdf","cScaleInv1":"#f4ffff","cScaleInv2":"#b2efc8","cScaleInv3":"#c0ada7","cScaleInv4":"#b0d0e4","cScaleInv5":"#91f5f5","cScaleInv6":"#c4ffb7","cScaleInv7":"#66a5fe","cScaleInv8":"#eab8f9","cScaleInv9":"#e9e8dd","cScaleInv10":"#ffd690","cScaleInv11":"#fe9d63","cScalePeer0":"hsl(180, 1.5873015873%, 22.3529411765%)","cScalePeer1":"hsl(0, 100%, 12.1568627451%)","cScalePeer2":"hsl(321.6393442623, 65.5913978495%, 28.2352941176%)","cScalePeer3":"hsl(194.4, 16.5562913907%, 39.6078431373%)","cScalePeer4":"hsl(23.0769230769, 49.0566037736%, 30.7843137255%)","cScalePeer5":"hsl(0, 83.3333333333%, 33.5294117647%)","cScalePeer6":"hsl(289.1666666667, 100%, 24.1176470588%)","cScalePeer7":"hsl(35.1315789474, 98.7012987013%, 40.1960784314%)","cScalePeer8":"hsl(106.1538461538, 84.4155844156%, 25.0980392157%)","cScalePeer9":"hsl(235, 21.4285714286%, 20.9803921569%)","cScalePeer10":"hsl(217.8378378378, 100%, 31.7647058824%)","cScalePeer11":"hsl(202.4516129032, 98.7261146497%, 40.7843137255%)","surface0":"hsl(210, 0%, 22.3529411765%)","surfacePeer0":"hsl(210, 0%, 19.3529411765%)","surface1":"hsl(210, 0%, 18.3529411765%)","surfacePeer1":"hsl(210, 0%, 15.3529411765%)","surface2":"hsl(210, 0%, 14.3529411765%)","surfacePeer2":"hsl(210, 0%, 11.3529411765%)","surface3":"hsl(210, 0%, 10.3529411765%)","surfacePeer3":"hsl(210, 0%, 7.3529411765%)","surface4":"hsl(210, 0%, 6.3529411765%)","surfacePeer4":"hsl(210, 0%, 3.3529411765%)","scaleLabelColor":"lightgrey","cScaleLabel0":"lightgrey","cScaleLabel1":"lightgrey","cScaleLabel2":"lightgrey","cScaleLabel3":"lightgrey","cScaleLabel4":"lightgrey","cScaleLabel5":"lightgrey","cScaleLabel6":"lightgrey","cScaleLabel7":"lightgrey","cScaleLabel8":"lightgrey","cScaleLabel9":"lightgrey","cScaleLabel10":"lightgrey","cScaleLabel11":"lightgrey","pie0":"#1f2020","pie1":"#0b0000","pie2":"#4d1037","pie3":"#3f5258","pie4":"#4f2f1b","pie5":"#6e0a0a","pie6":"#3b0048","pie7":"#995a01","pie8":"#154706","pie9":"#161722","pie10":"#00296f","pie11":"#01629c","pieTitleTextSize":"25px","pieTitleTextColor":"lightgrey","pieSectionTextSize":"17px","pieSectionTextColor":"#ccc","pieLegendTextSize":"17px","pieLegendTextColor":"lightgrey","pieStrokeColor":"black","pieStrokeWidth":"2px","pieOuterStrokeWidth":"2px","pieOuterStrokeColor":"black","pieOpacity":"0.7","venn1":"hsl(180, 1.5873015873%, 42.3529411765%)","venn2":"hsl(0, 100%, 32.1568627451%)","venn3":"hsl(321.6393442623, 65.5913978495%, 48.2352941176%)","venn4":"hsl(194.4, 16.5562913907%, 59.6078431373%)","venn5":"hsl(23.0769230769, 49.0566037736%, 50.7843137255%)","venn6":"hsl(0, 83.3333333333%, 53.5294117647%)","venn7":"hsl(289.1666666667, 100%, 44.1176470588%)","venn8":"hsl(35.1315789474, 98.7012987013%, 60.1960784314%)","vennTitleTextColor":"#F9FFFE","vennSetTextColor":"#ccc","cynefin":{"domainFontSize":16,"itemFontSize":12,"boundaryColor":"lightgrey","boundaryWidth":2,"cliffColor":"#FF6B6B","cliffWidth":4,"arrowColor":"lightgrey","arrowWidth":2,"complexBg":"#1B5E20","complicatedBg":"#0D47A1","chaoticBg":"#BF360C","clearBg":"#F57F17","confusionBg":"#4A148C","textColor":"#ccc","labelColor":"#e0dfdf"},"quadrant1Fill":"#1f2020","quadrant2Fill":"#242525","quadrant3Fill":"#292a2a","quadrant4Fill":"#2e2f2f","quadrant1TextFill":"#e0dfdf","quadrant2TextFill":"#dbdada","quadrant3TextFill":"#d6d5d5","quadrant4TextFill":"#d1d0d0","quadrantPointFill":"hsl(180, 1.5873015873%, NaN%)","quadrantPointTextFill":"#e0dfdf","quadrantXAxisTextFill":"#e0dfdf","quadrantYAxisTextFill":"#e0dfdf","quadrantInternalBorderStrokeFill":"#cccccc","quadrantExternalBorderStrokeFill":"#cccccc","quadrantTitleFill":"#e0dfdf","xyChart":{"backgroundColor":"#333","titleColor":"#e0dfdf","dataLabelColor":"#e0dfdf","legendTextColor":"#e0dfdf","xAxisTitleColor":"#e0dfdf","xAxisLabelColor":"#e0dfdf","xAxisTickColor":"#e0dfdf","xAxisLineColor":"#e0dfdf","yAxisTitleColor":"#e0dfdf","yAxisLabelColor":"#e0dfdf","yAxisTickColor":"#e0dfdf","yAxisLineColor":"#e0dfdf","plotColorPalette":"#3498db,#2ecc71,#e74c3c,#f1c40f,#bdc3c7,#ffffff,#34495e,#9b59b6,#1abc9c,#e67e22"},"packet":{"startByteColor":"#e0dfdf","endByteColor":"#e0dfdf","labelColor":"#e0dfdf","titleColor":"#e0dfdf","blockStrokeColor":"#e0dfdf","blockFillColor":"#333"},"radar":{"axisColor":"lightgrey","axisStrokeWidth":2,"axisLabelFontSize":12,"curveOpacity":0.5,"curveStrokeWidth":2,"graticuleColor":"#DEDEDE","graticuleStrokeWidth":1,"graticuleOpacity":0.3,"legendBoxSize":12,"legendFontSize":12},"wardleyEvolutionColor":"#ff6b6b","wardley":{"backgroundColor":"#333","axisColor":"lightgrey","axisTextColor":"#e0dfdf","gridColor":"lightgrey","componentFill":"#1f2020","componentStroke":"lightgrey","componentLabelColor":"#e0dfdf","linkStroke":"lightgrey","evolutionStroke":"#ff6b6b","annotationStroke":"lightgrey","annotationTextColor":"#e0dfdf","annotationFill":"#1f2020"},"classText":"#e0dfdf","requirementBackground":"#1f2020","requirementBorderColor":"#cccccc","requirementBorderSize":"1","requirementTextColor":"#e0dfdf","relationColor":"lightgrey","relationLabelBackground":"hsl(180, 1.5873015873%, 28.3529411765%)","relationLabelColor":"lightgrey","git0":"hsl(180, 1.5873015873%, 48.3529411765%)","git1":"hsl(321.6393442623, 65.5913978495%, 38.2352941176%)","git2":"hsl(194.4, 16.5562913907%, 49.6078431373%)","git3":"hsl(23.0769230769, 49.0566037736%, 40.7843137255%)","git4":"hsl(0, 83.3333333333%, 43.5294117647%)","git5":"hsl(289.1666666667, 100%, 24.1176470588%)","git6":"hsl(35.1315789474, 98.7012987013%, 40.1960784314%)","git7":"hsl(106.1538461538, 84.4155844156%, 35.0980392157%)","gitInv0":"rgb(133.6571428571, 129.7428571428, 129.7428571428)","gitInv1":"rgb(93.5483870969, 221.4516129033, 139.677419355)","gitInv2":"rgb(149.4437086091, 117.6092715231, 107.5562913906)","gitInv3":"rgb(99.9811320754, 162.7735849057, 202.0188679245)","gitInv4":"rgb(51.5000000001, 236.5, 236.5)","gitInv5":"rgb(154.2083333334, 255, 132.0000000001)","gitInv6":"rgb(51.331168831, 135.1948051946, 253.6688311688)","gitInv7":"rgb(206.1818181817, 89.948051948, 241.051948052)","gitBranchLabel0":"#2c2c2c","gitBranchLabel1":"lightgrey","gitBranchLabel2":"lightgrey","gitBranchLabel3":"#2c2c2c","gitBranchLabel4":"lightgrey","gitBranchLabel5":"lightgrey","gitBranchLabel6":"lightgrey","gitBranchLabel7":"lightgrey","tagLabelColor":"#e0dfdf","tagLabelBackground":"#1f2020","tagLabelBorder":"#cccccc","tagLabelFontSize":"10px","commitLabelColor":"rgb(183.8476190475, 181.5523809523, 181.5523809523)","commitLabelBackground":"hsl(180, 1.5873015873%, 28.3529411765%)","commitLabelFontSize":"10px","emUiFill":"#2d2d2d","emUiStroke":"#555","emProcessorFill":"hsl(296.1290322581, 20.2614379085%, 40%)","emProcessorStroke":"#8a6d8c","emReadModelFill":"hsl(98.6666666667, 33.3333333333%, 36.4705882353%)","emReadModelStroke":"#6d8c5c","emCommandFill":"hsl(218.6666666667, 33.3333333333%, 36.4705882353%)","emCommandStroke":"#5c6d8c","emEventFill":"hsl(32, 33.3333333333%, 36.4705882353%)","emEventStroke":"#8c755c","emSwimlaneBackgroundOdd":"hsl(0, 0%, 25%)","emSwimlaneBackgroundStroke":"hsl(0, 0%, 32%)","emArrowhead":"lightgrey","emRelationStroke":"lightgrey","attributeBackgroundColorOdd":"hsl(0, 0%, 32%)","attributeBackgroundColorEven":"hsl(0, 0%, 22%)"}"##;
    const DARK_COLORS: &str = r##"{"background":"#333","primaryColor":"#abcdef","secondaryColor":"#123456","tertiaryColor":"hsl(20, 1.5873015873%, 12.3529411765%)","primaryBorderColor":"#cccccc","secondaryBorderColor":"hsl(180, 0%, 18.3529411765%)","tertiaryBorderColor":"hsl(20, 0%, 2.3529411765%)","primaryTextColor":"#e0dfdf","secondaryTextColor":"rgb(183.8476190475, 181.5523809523, 181.5523809523)","tertiaryTextColor":"rgb(222.9999999999, 223.6666666666, 223.9999999999)","lineColor":"lightgrey","textColor":"#ccc","mainBkg":"#1f2020","secondBkg":"hsl(180, 1.5873015873%, 28.3529411765%)","mainContrastColor":"lightgrey","darkTextColor":"hsl(28.5714285714, 17.3553719008%, 86.2745098039%)","border1":"#ccc","border2":"rgba(255, 255, 255, 0.25)","arrowheadColor":"lightgrey","fontFamily":"\"trebuchet ms\", verdana, arial, sans-serif","fontSize":"16px","labelBackground":"#181818","THEME_COLOR_LIMIT":12,"radius":5,"strokeWidth":1,"nodeBkg":"#1f2020","nodeBorder":"#ccc","clusterBkg":"hsl(180, 1.5873015873%, 28.3529411765%)","clusterBorder":"rgba(255, 255, 255, 0.25)","defaultLinkColor":"lightgrey","titleColor":"#F9FFFE","edgeLabelBackground":"hsl(0, 0%, 34.4117647059%)","actorBorder":"#ccc","actorBkg":"#1f2020","actorTextColor":"lightgrey","actorLineColor":"#ccc","signalColor":"lightgrey","signalTextColor":"lightgrey","labelBoxBkgColor":"#1f2020","labelBoxBorderColor":"#ccc","labelTextColor":"lightgrey","loopTextColor":"lightgrey","noteBorderColor":"hsl(180, 0%, 18.3529411765%)","noteBkgColor":"hsl(180, 1.5873015873%, 28.3529411765%)","noteTextColor":"rgb(183.8476190475, 181.5523809523, 181.5523809523)","activationBorderColor":"#ccc","activationBkgColor":"hsl(180, 1.5873015873%, 28.3529411765%)","sequenceNumberColor":"black","sectionBkgColor":"hsl(52.9411764706, 28.813559322%, 58.431372549%)","altSectionBkgColor":"#333","sectionBkgColor2":"#EAE8D9","excludeBkgColor":"hsl(52.9411764706, 28.813559322%, 48.431372549%)","taskBorderColor":"#ffffff","taskBkgColor":"hsl(180, 1.5873015873%, 35.3529411765%)","taskTextColor":"hsl(28.5714285714, 17.3553719008%, 86.2745098039%)","taskTextLightColor":"lightgrey","taskTextOutsideColor":"lightgrey","taskTextClickableColor":"#003163","activeTaskBorderColor":"#ffffff","activeTaskBkgColor":"#81B1DB","gridColor":"lightgrey","doneTaskBkgColor":"lightgrey","doneTaskBorderColor":"grey","critBorderColor":"#E83737","critBkgColor":"#E83737","taskTextDarkColor":"#2c2c2c","todayLineColor":"#DB5757","vertLineColor":"#00BFFF","personBorder":"#cccccc","personBkg":"#1f2020","archEdgeColor":"lightgrey","archEdgeArrowColor":"lightgrey","archEdgeWidth":"3","archGroupBorderColor":"#cccccc","archGroupBorderWidth":"2px","rowOdd":"hsl(180, 1.5873015873%, 17.3529411765%)","rowEven":"hsl(180, 1.5873015873%, 2.3529411765%)","labelColor":"calculated","errorBkgColor":"#a44141","errorTextColor":"#ddd","useGradient":true,"gradientStart":"#cccccc","gradientStop":"hsl(180, 0%, 18.3529411765%)","dropShadow":"drop-shadow( 1px 2px 2px rgba(185,185,185,1))","noteFontWeight":"normal","fontWeight":"normal","darkMode":true,"flowContainerStroke":"hsl(180, 0%, 18.3529411765%)","rectBkgColor":"hsl(20, 1.5873015873%, 12.3529411765%)","transitionColor":"lightgrey","transitionLabelColor":"#ccc","stateLabelColor":"#e0dfdf","stateBkg":"#1f2020","labelBackgroundColor":"#1f2020","compositeBackground":"#333","altBackground":"#555","compositeTitleBackground":"#1f2020","compositeBorder":"#ccc","innerEndBackground":"#cccccc","specialStateColor":"#f4f4f4","fillType0":"#abcdef","fillType1":"#123456","fillType2":"hsl(274, 68%, 80.3921568627%)","fillType3":"hsl(274, 65.3846153846%, 20.3921568627%)","fillType4":"hsl(146, 68%, 80.3921568627%)","fillType5":"hsl(146, 65.3846153846%, 20.3921568627%)","fillType6":"hsl(338, 68%, 80.3921568627%)","fillType7":"hsl(338, 65.3846153846%, 20.3921568627%)","cScale1":"#0b0000","cScale2":"#4d1037","cScale3":"#3f5258","cScale4":"#4f2f1b","cScale5":"#6e0a0a","cScale6":"#3b0048","cScale7":"#995a01","cScale8":"#154706","cScale9":"#161722","cScale10":"#00296f","cScale11":"#01629c","cScale12":"#010029","cScale0":"#abcdef","cScaleInv0":"#543210","cScaleInv1":"#f4ffff","cScaleInv2":"#b2efc8","cScaleInv3":"#c0ada7","cScaleInv4":"#b0d0e4","cScaleInv5":"#91f5f5","cScaleInv6":"#c4ffb7","cScaleInv7":"#66a5fe","cScaleInv8":"#eab8f9","cScaleInv9":"#e9e8dd","cScaleInv10":"#ffd690","cScaleInv11":"#fe9d63","cScalePeer0":"hsl(210, 68%, 90.3921568627%)","cScalePeer1":"hsl(0, 100%, 12.1568627451%)","cScalePeer2":"hsl(321.6393442623, 65.5913978495%, 28.2352941176%)","cScalePeer3":"hsl(194.4, 16.5562913907%, 39.6078431373%)","cScalePeer4":"hsl(23.0769230769, 49.0566037736%, 30.7843137255%)","cScalePeer5":"hsl(0, 83.3333333333%, 33.5294117647%)","cScalePeer6":"hsl(289.1666666667, 100%, 24.1176470588%)","cScalePeer7":"hsl(35.1315789474, 98.7012987013%, 40.1960784314%)","cScalePeer8":"hsl(106.1538461538, 84.4155844156%, 25.0980392157%)","cScalePeer9":"hsl(235, 21.4285714286%, 20.9803921569%)","cScalePeer10":"hsl(217.8378378378, 100%, 31.7647058824%)","cScalePeer11":"hsl(202.4516129032, 98.7261146497%, 40.7843137255%)","surface0":"hsl(210, 0%, 22.3529411765%)","surfacePeer0":"hsl(210, 0%, 19.3529411765%)","surface1":"hsl(210, 0%, 18.3529411765%)","surfacePeer1":"hsl(210, 0%, 15.3529411765%)","surface2":"hsl(210, 0%, 14.3529411765%)","surfacePeer2":"hsl(210, 0%, 11.3529411765%)","surface3":"hsl(210, 0%, 10.3529411765%)","surfacePeer3":"hsl(210, 0%, 7.3529411765%)","surface4":"hsl(210, 0%, 6.3529411765%)","surfacePeer4":"hsl(210, 0%, 3.3529411765%)","scaleLabelColor":"black","cScaleLabel0":"black","cScaleLabel1":"black","cScaleLabel2":"black","cScaleLabel3":"black","cScaleLabel4":"black","cScaleLabel5":"black","cScaleLabel6":"black","cScaleLabel7":"black","cScaleLabel8":"black","cScaleLabel9":"black","cScaleLabel10":"black","cScaleLabel11":"black","pie0":"#abcdef","pie1":"#0b0000","pie2":"#4d1037","pie3":"#3f5258","pie4":"#4f2f1b","pie5":"#6e0a0a","pie6":"#3b0048","pie7":"#995a01","pie8":"#154706","pie9":"#161722","pie10":"#00296f","pie11":"#01629c","pieTitleTextSize":"25px","pieTitleTextColor":"lightgrey","pieSectionTextSize":"17px","pieSectionTextColor":"#ccc","pieLegendTextSize":"17px","pieLegendTextColor":"lightgrey","pieStrokeColor":"black","pieStrokeWidth":"2px","pieOuterStrokeWidth":"2px","pieOuterStrokeColor":"black","pieOpacity":"0.7","venn1":"hsl(210, 68%, 100%)","venn2":"hsl(0, 100%, 32.1568627451%)","venn3":"hsl(321.6393442623, 65.5913978495%, 48.2352941176%)","venn4":"hsl(194.4, 16.5562913907%, 59.6078431373%)","venn5":"hsl(23.0769230769, 49.0566037736%, 50.7843137255%)","venn6":"hsl(0, 83.3333333333%, 53.5294117647%)","venn7":"hsl(289.1666666667, 100%, 44.1176470588%)","venn8":"hsl(35.1315789474, 98.7012987013%, 60.1960784314%)","vennTitleTextColor":"#F9FFFE","vennSetTextColor":"#ccc","cynefin":{"domainFontSize":16,"itemFontSize":12,"boundaryColor":"lightgrey","boundaryWidth":2,"cliffColor":"#FF6B6B","cliffWidth":4,"arrowColor":"lightgrey","arrowWidth":2,"complexBg":"#1B5E20","complicatedBg":"#0D47A1","chaoticBg":"#BF360C","clearBg":"#F57F17","confusionBg":"#4A148C","textColor":"#ccc","labelColor":"#e0dfdf"},"quadrant1Fill":"#abcdef","quadrant2Fill":"#b0d2f4","quadrant3Fill":"#b5d7f9","quadrant4Fill":"#badcfe","quadrant1TextFill":"#e0dfdf","quadrant2TextFill":"#dbdada","quadrant3TextFill":"#d6d5d5","quadrant4TextFill":"#d1d0d0","quadrantPointFill":"hsl(210, 68%, NaN%)","quadrantPointTextFill":"#e0dfdf","quadrantXAxisTextFill":"#e0dfdf","quadrantYAxisTextFill":"#e0dfdf","quadrantInternalBorderStrokeFill":"#cccccc","quadrantExternalBorderStrokeFill":"#cccccc","quadrantTitleFill":"#e0dfdf","xyChart":{"backgroundColor":"#333","titleColor":"#e0dfdf","dataLabelColor":"#e0dfdf","legendTextColor":"#e0dfdf","xAxisTitleColor":"#e0dfdf","xAxisLabelColor":"#e0dfdf","xAxisTickColor":"#e0dfdf","xAxisLineColor":"#e0dfdf","yAxisTitleColor":"#e0dfdf","yAxisLabelColor":"#e0dfdf","yAxisTickColor":"#e0dfdf","yAxisLineColor":"#e0dfdf","plotColorPalette":"#3498db,#2ecc71,#e74c3c,#f1c40f,#bdc3c7,#ffffff,#34495e,#9b59b6,#1abc9c,#e67e22"},"packet":{"startByteColor":"#e0dfdf","endByteColor":"#e0dfdf","labelColor":"#e0dfdf","titleColor":"#e0dfdf","blockStrokeColor":"#e0dfdf","blockFillColor":"#333"},"radar":{"axisColor":"lightgrey","axisStrokeWidth":2,"axisLabelFontSize":12,"curveOpacity":0.5,"curveStrokeWidth":2,"graticuleColor":"#DEDEDE","graticuleStrokeWidth":1,"graticuleOpacity":0.3,"legendBoxSize":12,"legendFontSize":12},"wardleyEvolutionColor":"#ff6b6b","wardley":{"backgroundColor":"#333","axisColor":"lightgrey","axisTextColor":"#e0dfdf","gridColor":"lightgrey","componentFill":"#1f2020","componentStroke":"lightgrey","componentLabelColor":"#e0dfdf","linkStroke":"lightgrey","evolutionStroke":"#ff6b6b","annotationStroke":"lightgrey","annotationTextColor":"#e0dfdf","annotationFill":"#1f2020"},"classText":"#e0dfdf","requirementBackground":"#abcdef","requirementBorderColor":"#cccccc","requirementBorderSize":"1","requirementTextColor":"#e0dfdf","relationColor":"lightgrey","relationLabelBackground":"hsl(210, 65.3846153846%, 0%)","relationLabelColor":"lightgrey","git0":"hsl(210, 65.3846153846%, 40.3921568627%)","git1":"hsl(321.6393442623, 65.5913978495%, 38.2352941176%)","git2":"hsl(194.4, 16.5562913907%, 49.6078431373%)","git3":"hsl(23.0769230769, 49.0566037736%, 40.7843137255%)","git4":"hsl(0, 83.3333333333%, 43.5294117647%)","git5":"hsl(289.1666666667, 100%, 24.1176470588%)","git6":"hsl(35.1315789474, 98.7012987013%, 40.1960784314%)","git7":"hsl(106.1538461538, 84.4155844156%, 35.0980392157%)","gitInv0":"rgb(219.3461538462, 152.0000000001, 84.6538461541)","gitInv1":"rgb(93.5483870969, 221.4516129033, 139.677419355)","gitInv2":"rgb(149.4437086091, 117.6092715231, 107.5562913906)","gitInv3":"rgb(99.9811320754, 162.7735849057, 202.0188679245)","gitInv4":"rgb(51.5000000001, 236.5, 236.5)","gitInv5":"rgb(154.2083333334, 255, 132.0000000001)","gitInv6":"rgb(51.331168831, 135.1948051946, 253.6688311688)","gitInv7":"rgb(206.1818181817, 89.948051948, 241.051948052)","gitBranchLabel0":"#2c2c2c","gitBranchLabel1":"lightgrey","gitBranchLabel2":"lightgrey","gitBranchLabel3":"#2c2c2c","gitBranchLabel4":"lightgrey","gitBranchLabel5":"lightgrey","gitBranchLabel6":"lightgrey","gitBranchLabel7":"lightgrey","tagLabelColor":"#e0dfdf","tagLabelBackground":"#abcdef","tagLabelBorder":"#cccccc","tagLabelFontSize":"10px","commitLabelColor":"rgb(183.8476190475, 181.5523809523, 181.5523809523)","commitLabelBackground":"#123456","commitLabelFontSize":"10px","emUiFill":"#2d2d2d","emUiStroke":"#555","emProcessorFill":"hsl(296.1290322581, 20.2614379085%, 40%)","emProcessorStroke":"#8a6d8c","emReadModelFill":"hsl(98.6666666667, 33.3333333333%, 36.4705882353%)","emReadModelStroke":"#6d8c5c","emCommandFill":"hsl(218.6666666667, 33.3333333333%, 36.4705882353%)","emCommandStroke":"#5c6d8c","emEventFill":"hsl(32, 33.3333333333%, 36.4705882353%)","emEventStroke":"#8c755c","emSwimlaneBackgroundOdd":"hsl(0, 0%, 25%)","emSwimlaneBackgroundStroke":"hsl(0, 0%, 32%)","emArrowhead":"lightgrey","emRelationStroke":"lightgrey","attributeBackgroundColorOdd":"hsl(0, 0%, 32%)","attributeBackgroundColorEven":"hsl(0, 0%, 22%)"}"##;
    const DARK_NESTED: &str = r##"{"background":"#333","primaryColor":"#1f2020","secondaryColor":"hsl(180, 1.5873015873%, 28.3529411765%)","tertiaryColor":"hsl(20, 1.5873015873%, 12.3529411765%)","primaryBorderColor":"#cccccc","secondaryBorderColor":"hsl(180, 0%, 18.3529411765%)","tertiaryBorderColor":"hsl(20, 0%, 2.3529411765%)","primaryTextColor":"#e0dfdf","secondaryTextColor":"rgb(183.8476190475, 181.5523809523, 181.5523809523)","tertiaryTextColor":"rgb(222.9999999999, 223.6666666666, 223.9999999999)","lineColor":"lightgrey","textColor":"#ccc","mainBkg":"#1f2020","secondBkg":"hsl(180, 1.5873015873%, 28.3529411765%)","mainContrastColor":"lightgrey","darkTextColor":"hsl(28.5714285714, 17.3553719008%, 86.2745098039%)","border1":"#ccc","border2":"rgba(255, 255, 255, 0.25)","arrowheadColor":"lightgrey","fontFamily":"\"trebuchet ms\", verdana, arial, sans-serif","fontSize":"16px","labelBackground":"#181818","THEME_COLOR_LIMIT":12,"radius":5,"strokeWidth":1,"nodeBkg":"#1f2020","nodeBorder":"#ccc","clusterBkg":"hsl(180, 1.5873015873%, 28.3529411765%)","clusterBorder":"rgba(255, 255, 255, 0.25)","defaultLinkColor":"lightgrey","titleColor":"#F9FFFE","edgeLabelBackground":"hsl(0, 0%, 34.4117647059%)","actorBorder":"#ccc","actorBkg":"#1f2020","actorTextColor":"lightgrey","actorLineColor":"#ccc","signalColor":"lightgrey","signalTextColor":"lightgrey","labelBoxBkgColor":"#1f2020","labelBoxBorderColor":"#ccc","labelTextColor":"lightgrey","loopTextColor":"lightgrey","noteBorderColor":"hsl(180, 0%, 18.3529411765%)","noteBkgColor":"hsl(180, 1.5873015873%, 28.3529411765%)","noteTextColor":"rgb(183.8476190475, 181.5523809523, 181.5523809523)","activationBorderColor":"#ccc","activationBkgColor":"hsl(180, 1.5873015873%, 28.3529411765%)","sequenceNumberColor":"black","sectionBkgColor":"hsl(52.9411764706, 28.813559322%, 58.431372549%)","altSectionBkgColor":"#333","sectionBkgColor2":"#EAE8D9","excludeBkgColor":"hsl(52.9411764706, 28.813559322%, 48.431372549%)","taskBorderColor":"#ffffff","taskBkgColor":"hsl(180, 1.5873015873%, 35.3529411765%)","taskTextColor":"hsl(28.5714285714, 17.3553719008%, 86.2745098039%)","taskTextLightColor":"lightgrey","taskTextOutsideColor":"lightgrey","taskTextClickableColor":"#003163","activeTaskBorderColor":"#ffffff","activeTaskBkgColor":"#81B1DB","gridColor":"lightgrey","doneTaskBkgColor":"lightgrey","doneTaskBorderColor":"grey","critBorderColor":"#E83737","critBkgColor":"#E83737","taskTextDarkColor":"#2c2c2c","todayLineColor":"#DB5757","vertLineColor":"#00BFFF","personBorder":"#cccccc","personBkg":"#1f2020","archEdgeColor":"lightgrey","archEdgeArrowColor":"lightgrey","archEdgeWidth":"3","archGroupBorderColor":"#cccccc","archGroupBorderWidth":"2px","rowOdd":"hsl(180, 1.5873015873%, 17.3529411765%)","rowEven":"hsl(180, 1.5873015873%, 2.3529411765%)","labelColor":"calculated","errorBkgColor":"#a44141","errorTextColor":"#ddd","useGradient":true,"gradientStart":"#cccccc","gradientStop":"hsl(180, 0%, 18.3529411765%)","dropShadow":"drop-shadow( 1px 2px 2px rgba(185,185,185,1))","noteFontWeight":"normal","fontWeight":"normal","xyChart":{"backgroundColor":"#333","titleColor":"#ff0000","dataLabelColor":"#e0dfdf","legendTextColor":"#e0dfdf","xAxisTitleColor":"#e0dfdf","xAxisLabelColor":"#e0dfdf","xAxisTickColor":"#e0dfdf","xAxisLineColor":"#e0dfdf","yAxisTitleColor":"#e0dfdf","yAxisLabelColor":"#e0dfdf","yAxisTickColor":"#e0dfdf","yAxisLineColor":"#e0dfdf","plotColorPalette":"#3498db,#2ecc71,#e74c3c,#f1c40f,#bdc3c7,#ffffff,#34495e,#9b59b6,#1abc9c,#e67e22"},"flowContainerStroke":"hsl(180, 0%, 18.3529411765%)","rectBkgColor":"hsl(20, 1.5873015873%, 12.3529411765%)","transitionColor":"lightgrey","transitionLabelColor":"#ccc","stateLabelColor":"#e0dfdf","stateBkg":"#1f2020","labelBackgroundColor":"#1f2020","compositeBackground":"#333","altBackground":"#555","compositeTitleBackground":"#1f2020","compositeBorder":"#ccc","innerEndBackground":"#cccccc","specialStateColor":"#f4f4f4","fillType0":"#1f2020","fillType1":"hsl(180, 1.5873015873%, 28.3529411765%)","fillType2":"hsl(244, 1.5873015873%, 12.3529411765%)","fillType3":"hsl(244, 1.5873015873%, 28.3529411765%)","fillType4":"hsl(116, 1.5873015873%, 12.3529411765%)","fillType5":"hsl(116, 1.5873015873%, 28.3529411765%)","fillType6":"hsl(308, 1.5873015873%, 12.3529411765%)","fillType7":"hsl(308, 1.5873015873%, 28.3529411765%)","cScale1":"#0b0000","cScale2":"#4d1037","cScale3":"#3f5258","cScale4":"#4f2f1b","cScale5":"#6e0a0a","cScale6":"#3b0048","cScale7":"#995a01","cScale8":"#154706","cScale9":"#161722","cScale10":"#00296f","cScale11":"#01629c","cScale12":"#010029","cScale0":"#1f2020","cScaleInv0":"#e0dfdf","cScaleInv1":"#f4ffff","cScaleInv2":"#b2efc8","cScaleInv3":"#c0ada7","cScaleInv4":"#b0d0e4","cScaleInv5":"#91f5f5","cScaleInv6":"#c4ffb7","cScaleInv7":"#66a5fe","cScaleInv8":"#eab8f9","cScaleInv9":"#e9e8dd","cScaleInv10":"#ffd690","cScaleInv11":"#fe9d63","cScalePeer0":"hsl(180, 1.5873015873%, 22.3529411765%)","cScalePeer1":"hsl(0, 100%, 12.1568627451%)","cScalePeer2":"hsl(321.6393442623, 65.5913978495%, 28.2352941176%)","cScalePeer3":"hsl(194.4, 16.5562913907%, 39.6078431373%)","cScalePeer4":"hsl(23.0769230769, 49.0566037736%, 30.7843137255%)","cScalePeer5":"hsl(0, 83.3333333333%, 33.5294117647%)","cScalePeer6":"hsl(289.1666666667, 100%, 24.1176470588%)","cScalePeer7":"hsl(35.1315789474, 98.7012987013%, 40.1960784314%)","cScalePeer8":"hsl(106.1538461538, 84.4155844156%, 25.0980392157%)","cScalePeer9":"hsl(235, 21.4285714286%, 20.9803921569%)","cScalePeer10":"hsl(217.8378378378, 100%, 31.7647058824%)","cScalePeer11":"hsl(202.4516129032, 98.7261146497%, 40.7843137255%)","surface0":"hsl(210, 0%, 22.3529411765%)","surfacePeer0":"hsl(210, 0%, 19.3529411765%)","surface1":"hsl(210, 0%, 18.3529411765%)","surfacePeer1":"hsl(210, 0%, 15.3529411765%)","surface2":"hsl(210, 0%, 14.3529411765%)","surfacePeer2":"hsl(210, 0%, 11.3529411765%)","surface3":"hsl(210, 0%, 10.3529411765%)","surfacePeer3":"hsl(210, 0%, 7.3529411765%)","surface4":"hsl(210, 0%, 6.3529411765%)","surfacePeer4":"hsl(210, 0%, 3.3529411765%)","scaleLabelColor":"lightgrey","cScaleLabel0":"lightgrey","cScaleLabel1":"lightgrey","cScaleLabel2":"lightgrey","cScaleLabel3":"lightgrey","cScaleLabel4":"lightgrey","cScaleLabel5":"lightgrey","cScaleLabel6":"lightgrey","cScaleLabel7":"lightgrey","cScaleLabel8":"lightgrey","cScaleLabel9":"lightgrey","cScaleLabel10":"lightgrey","cScaleLabel11":"lightgrey","pie0":"#1f2020","pie1":"#0b0000","pie2":"#4d1037","pie3":"#3f5258","pie4":"#4f2f1b","pie5":"#6e0a0a","pie6":"#3b0048","pie7":"#995a01","pie8":"#154706","pie9":"#161722","pie10":"#00296f","pie11":"#01629c","pieTitleTextSize":"25px","pieTitleTextColor":"lightgrey","pieSectionTextSize":"17px","pieSectionTextColor":"#ccc","pieLegendTextSize":"17px","pieLegendTextColor":"lightgrey","pieStrokeColor":"black","pieStrokeWidth":"2px","pieOuterStrokeWidth":"2px","pieOuterStrokeColor":"black","pieOpacity":"0.7","venn1":"hsl(180, 1.5873015873%, 42.3529411765%)","venn2":"hsl(0, 100%, 32.1568627451%)","venn3":"hsl(321.6393442623, 65.5913978495%, 48.2352941176%)","venn4":"hsl(194.4, 16.5562913907%, 59.6078431373%)","venn5":"hsl(23.0769230769, 49.0566037736%, 50.7843137255%)","venn6":"hsl(0, 83.3333333333%, 53.5294117647%)","venn7":"hsl(289.1666666667, 100%, 44.1176470588%)","venn8":"hsl(35.1315789474, 98.7012987013%, 60.1960784314%)","vennTitleTextColor":"#F9FFFE","vennSetTextColor":"#ccc","cynefin":{"domainFontSize":16,"itemFontSize":12,"boundaryColor":"lightgrey","boundaryWidth":2,"cliffColor":"#FF6B6B","cliffWidth":4,"arrowColor":"lightgrey","arrowWidth":2,"complexBg":"#1B5E20","complicatedBg":"#0D47A1","chaoticBg":"#BF360C","clearBg":"#F57F17","confusionBg":"#4A148C","textColor":"#ccc","labelColor":"#e0dfdf"},"quadrant1Fill":"#1f2020","quadrant2Fill":"#242525","quadrant3Fill":"#292a2a","quadrant4Fill":"#2e2f2f","quadrant1TextFill":"#e0dfdf","quadrant2TextFill":"#dbdada","quadrant3TextFill":"#d6d5d5","quadrant4TextFill":"#d1d0d0","quadrantPointFill":"hsl(180, 1.5873015873%, NaN%)","quadrantPointTextFill":"#e0dfdf","quadrantXAxisTextFill":"#e0dfdf","quadrantYAxisTextFill":"#e0dfdf","quadrantInternalBorderStrokeFill":"#cccccc","quadrantExternalBorderStrokeFill":"#cccccc","quadrantTitleFill":"#e0dfdf","packet":{"startByteColor":"#e0dfdf","endByteColor":"#e0dfdf","labelColor":"#e0dfdf","titleColor":"#e0dfdf","blockStrokeColor":"#e0dfdf","blockFillColor":"#333"},"radar":{"axisColor":"lightgrey","axisStrokeWidth":2,"axisLabelFontSize":12,"curveOpacity":0.5,"curveStrokeWidth":2,"graticuleColor":"#DEDEDE","graticuleStrokeWidth":1,"graticuleOpacity":0.3,"legendBoxSize":12,"legendFontSize":12},"wardleyEvolutionColor":"#ff6b6b","wardley":{"backgroundColor":"#333","axisColor":"lightgrey","axisTextColor":"#e0dfdf","gridColor":"lightgrey","componentFill":"#1f2020","componentStroke":"lightgrey","componentLabelColor":"#e0dfdf","linkStroke":"lightgrey","evolutionStroke":"#ff6b6b","annotationStroke":"lightgrey","annotationTextColor":"#e0dfdf","annotationFill":"#1f2020"},"classText":"#e0dfdf","requirementBackground":"#1f2020","requirementBorderColor":"#cccccc","requirementBorderSize":"1","requirementTextColor":"#e0dfdf","relationColor":"lightgrey","relationLabelBackground":"hsl(180, 1.5873015873%, 28.3529411765%)","relationLabelColor":"lightgrey","git0":"hsl(180, 1.5873015873%, 48.3529411765%)","git1":"hsl(321.6393442623, 65.5913978495%, 38.2352941176%)","git2":"hsl(194.4, 16.5562913907%, 49.6078431373%)","git3":"hsl(23.0769230769, 49.0566037736%, 40.7843137255%)","git4":"hsl(0, 83.3333333333%, 43.5294117647%)","git5":"hsl(289.1666666667, 100%, 24.1176470588%)","git6":"hsl(35.1315789474, 98.7012987013%, 40.1960784314%)","git7":"hsl(106.1538461538, 84.4155844156%, 35.0980392157%)","gitInv0":"rgb(133.6571428571, 129.7428571428, 129.7428571428)","gitInv1":"rgb(93.5483870969, 221.4516129033, 139.677419355)","gitInv2":"rgb(149.4437086091, 117.6092715231, 107.5562913906)","gitInv3":"rgb(99.9811320754, 162.7735849057, 202.0188679245)","gitInv4":"rgb(51.5000000001, 236.5, 236.5)","gitInv5":"rgb(154.2083333334, 255, 132.0000000001)","gitInv6":"rgb(51.331168831, 135.1948051946, 253.6688311688)","gitInv7":"rgb(206.1818181817, 89.948051948, 241.051948052)","gitBranchLabel0":"#2c2c2c","gitBranchLabel1":"lightgrey","gitBranchLabel2":"lightgrey","gitBranchLabel3":"#2c2c2c","gitBranchLabel4":"lightgrey","gitBranchLabel5":"lightgrey","gitBranchLabel6":"lightgrey","gitBranchLabel7":"lightgrey","tagLabelColor":"#e0dfdf","tagLabelBackground":"#1f2020","tagLabelBorder":"#cccccc","tagLabelFontSize":"10px","commitLabelColor":"rgb(183.8476190475, 181.5523809523, 181.5523809523)","commitLabelBackground":"hsl(180, 1.5873015873%, 28.3529411765%)","commitLabelFontSize":"10px","emUiFill":"#2d2d2d","emUiStroke":"#555","emProcessorFill":"hsl(296.1290322581, 20.2614379085%, 40%)","emProcessorStroke":"#8a6d8c","emReadModelFill":"hsl(98.6666666667, 33.3333333333%, 36.4705882353%)","emReadModelStroke":"#6d8c5c","emCommandFill":"hsl(218.6666666667, 33.3333333333%, 36.4705882353%)","emCommandStroke":"#5c6d8c","emEventFill":"hsl(32, 33.3333333333%, 36.4705882353%)","emEventStroke":"#8c755c","emSwimlaneBackgroundOdd":"hsl(0, 0%, 25%)","emSwimlaneBackgroundStroke":"hsl(0, 0%, 32%)","emArrowhead":"lightgrey","emRelationStroke":"lightgrey","attributeBackgroundColorOdd":"hsl(0, 0%, 32%)","attributeBackgroundColorEven":"hsl(0, 0%, 22%)"}"##;
    const DARK_BORDER: &str = r##"{"background":"#333","primaryColor":"#1f2020","secondaryColor":"hsl(180, 1.5873015873%, 28.3529411765%)","tertiaryColor":"hsl(20, 1.5873015873%, 12.3529411765%)","primaryBorderColor":"#cccccc","secondaryBorderColor":"hsl(180, 0%, 18.3529411765%)","tertiaryBorderColor":"hsl(20, 0%, 2.3529411765%)","primaryTextColor":"#e0dfdf","secondaryTextColor":"rgb(183.8476190475, 181.5523809523, 181.5523809523)","tertiaryTextColor":"rgb(222.9999999999, 223.6666666666, 223.9999999999)","lineColor":"lightgrey","textColor":"#ccc","mainBkg":"#1f2020","secondBkg":"hsl(180, 1.5873015873%, 28.3529411765%)","mainContrastColor":"lightgrey","darkTextColor":"hsl(28.5714285714, 17.3553719008%, 86.2745098039%)","border1":"#ccc","border2":"rgba(255, 255, 255, 0.25)","arrowheadColor":"lightgrey","fontFamily":"Arial","fontSize":"16px","labelBackground":"#181818","THEME_COLOR_LIMIT":12,"radius":5,"strokeWidth":1,"nodeBkg":"#1f2020","nodeBorder":"#225577","clusterBkg":"hsl(180, 1.5873015873%, 28.3529411765%)","clusterBorder":"rgba(255, 255, 255, 0.25)","defaultLinkColor":"lightgrey","titleColor":"#F9FFFE","edgeLabelBackground":"hsl(0, 0%, 34.4117647059%)","actorBorder":"#ccc","actorBkg":"#1f2020","actorTextColor":"lightgrey","actorLineColor":"#ccc","signalColor":"lightgrey","signalTextColor":"lightgrey","labelBoxBkgColor":"#1f2020","labelBoxBorderColor":"#ccc","labelTextColor":"lightgrey","loopTextColor":"lightgrey","noteBorderColor":"hsl(180, 0%, 18.3529411765%)","noteBkgColor":"hsl(180, 1.5873015873%, 28.3529411765%)","noteTextColor":"rgb(183.8476190475, 181.5523809523, 181.5523809523)","activationBorderColor":"#ccc","activationBkgColor":"hsl(180, 1.5873015873%, 28.3529411765%)","sequenceNumberColor":"black","sectionBkgColor":"hsl(52.9411764706, 28.813559322%, 58.431372549%)","altSectionBkgColor":"#333","sectionBkgColor2":"#EAE8D9","excludeBkgColor":"hsl(52.9411764706, 28.813559322%, 48.431372549%)","taskBorderColor":"#ffffff","taskBkgColor":"hsl(180, 1.5873015873%, 35.3529411765%)","taskTextColor":"hsl(28.5714285714, 17.3553719008%, 86.2745098039%)","taskTextLightColor":"lightgrey","taskTextOutsideColor":"lightgrey","taskTextClickableColor":"#003163","activeTaskBorderColor":"#ffffff","activeTaskBkgColor":"#81B1DB","gridColor":"lightgrey","doneTaskBkgColor":"lightgrey","doneTaskBorderColor":"grey","critBorderColor":"#E83737","critBkgColor":"#E83737","taskTextDarkColor":"#2c2c2c","todayLineColor":"#DB5757","vertLineColor":"#00BFFF","personBorder":"#cccccc","personBkg":"#1f2020","archEdgeColor":"lightgrey","archEdgeArrowColor":"lightgrey","archEdgeWidth":"3","archGroupBorderColor":"#cccccc","archGroupBorderWidth":"2px","rowOdd":"hsl(180, 1.5873015873%, 17.3529411765%)","rowEven":"hsl(180, 1.5873015873%, 2.3529411765%)","labelColor":"calculated","errorBkgColor":"#a44141","errorTextColor":"#ddd","useGradient":true,"gradientStart":"#cccccc","gradientStop":"hsl(180, 0%, 18.3529411765%)","dropShadow":"drop-shadow( 1px 2px 2px rgba(185,185,185,1))","noteFontWeight":"normal","fontWeight":"normal","cScale0":"#123456","flowContainerStroke":"hsl(180, 0%, 18.3529411765%)","rectBkgColor":"hsl(20, 1.5873015873%, 12.3529411765%)","transitionColor":"lightgrey","transitionLabelColor":"#ccc","stateLabelColor":"#e0dfdf","stateBkg":"#1f2020","labelBackgroundColor":"#1f2020","compositeBackground":"#333","altBackground":"#555","compositeTitleBackground":"#1f2020","compositeBorder":"#ccc","innerEndBackground":"#cccccc","specialStateColor":"#f4f4f4","fillType0":"#1f2020","fillType1":"hsl(180, 1.5873015873%, 28.3529411765%)","fillType2":"hsl(244, 1.5873015873%, 12.3529411765%)","fillType3":"hsl(244, 1.5873015873%, 28.3529411765%)","fillType4":"hsl(116, 1.5873015873%, 12.3529411765%)","fillType5":"hsl(116, 1.5873015873%, 28.3529411765%)","fillType6":"hsl(308, 1.5873015873%, 12.3529411765%)","fillType7":"hsl(308, 1.5873015873%, 28.3529411765%)","cScale1":"#0b0000","cScale2":"#4d1037","cScale3":"#3f5258","cScale4":"#4f2f1b","cScale5":"#6e0a0a","cScale6":"#3b0048","cScale7":"#995a01","cScale8":"#154706","cScale9":"#161722","cScale10":"#00296f","cScale11":"#01629c","cScale12":"#010029","cScaleInv0":"#edcba9","cScaleInv1":"#f4ffff","cScaleInv2":"#b2efc8","cScaleInv3":"#c0ada7","cScaleInv4":"#b0d0e4","cScaleInv5":"#91f5f5","cScaleInv6":"#c4ffb7","cScaleInv7":"#66a5fe","cScaleInv8":"#eab8f9","cScaleInv9":"#e9e8dd","cScaleInv10":"#ffd690","cScaleInv11":"#fe9d63","cScalePeer0":"hsl(210, 65.3846153846%, 30.3921568627%)","cScalePeer1":"hsl(0, 100%, 12.1568627451%)","cScalePeer2":"hsl(321.6393442623, 65.5913978495%, 28.2352941176%)","cScalePeer3":"hsl(194.4, 16.5562913907%, 39.6078431373%)","cScalePeer4":"hsl(23.0769230769, 49.0566037736%, 30.7843137255%)","cScalePeer5":"hsl(0, 83.3333333333%, 33.5294117647%)","cScalePeer6":"hsl(289.1666666667, 100%, 24.1176470588%)","cScalePeer7":"hsl(35.1315789474, 98.7012987013%, 40.1960784314%)","cScalePeer8":"hsl(106.1538461538, 84.4155844156%, 25.0980392157%)","cScalePeer9":"hsl(235, 21.4285714286%, 20.9803921569%)","cScalePeer10":"hsl(217.8378378378, 100%, 31.7647058824%)","cScalePeer11":"hsl(202.4516129032, 98.7261146497%, 40.7843137255%)","surface0":"hsl(210, 0%, 22.3529411765%)","surfacePeer0":"hsl(210, 0%, 19.3529411765%)","surface1":"hsl(210, 0%, 18.3529411765%)","surfacePeer1":"hsl(210, 0%, 15.3529411765%)","surface2":"hsl(210, 0%, 14.3529411765%)","surfacePeer2":"hsl(210, 0%, 11.3529411765%)","surface3":"hsl(210, 0%, 10.3529411765%)","surfacePeer3":"hsl(210, 0%, 7.3529411765%)","surface4":"hsl(210, 0%, 6.3529411765%)","surfacePeer4":"hsl(210, 0%, 3.3529411765%)","scaleLabelColor":"lightgrey","cScaleLabel0":"lightgrey","cScaleLabel1":"lightgrey","cScaleLabel2":"lightgrey","cScaleLabel3":"lightgrey","cScaleLabel4":"lightgrey","cScaleLabel5":"lightgrey","cScaleLabel6":"lightgrey","cScaleLabel7":"lightgrey","cScaleLabel8":"lightgrey","cScaleLabel9":"lightgrey","cScaleLabel10":"lightgrey","cScaleLabel11":"lightgrey","pie0":"#123456","pie1":"#0b0000","pie2":"#4d1037","pie3":"#3f5258","pie4":"#4f2f1b","pie5":"#6e0a0a","pie6":"#3b0048","pie7":"#995a01","pie8":"#154706","pie9":"#161722","pie10":"#00296f","pie11":"#01629c","pieTitleTextSize":"25px","pieTitleTextColor":"lightgrey","pieSectionTextSize":"17px","pieSectionTextColor":"#ccc","pieLegendTextSize":"17px","pieLegendTextColor":"lightgrey","pieStrokeColor":"black","pieStrokeWidth":"2px","pieOuterStrokeWidth":"2px","pieOuterStrokeColor":"black","pieOpacity":"0.7","venn1":"hsl(210, 65.3846153846%, 50.3921568627%)","venn2":"hsl(0, 100%, 32.1568627451%)","venn3":"hsl(321.6393442623, 65.5913978495%, 48.2352941176%)","venn4":"hsl(194.4, 16.5562913907%, 59.6078431373%)","venn5":"hsl(23.0769230769, 49.0566037736%, 50.7843137255%)","venn6":"hsl(0, 83.3333333333%, 53.5294117647%)","venn7":"hsl(289.1666666667, 100%, 44.1176470588%)","venn8":"hsl(35.1315789474, 98.7012987013%, 60.1960784314%)","vennTitleTextColor":"#F9FFFE","vennSetTextColor":"#ccc","cynefin":{"domainFontSize":16,"itemFontSize":12,"boundaryColor":"lightgrey","boundaryWidth":2,"cliffColor":"#FF6B6B","cliffWidth":4,"arrowColor":"lightgrey","arrowWidth":2,"complexBg":"#1B5E20","complicatedBg":"#0D47A1","chaoticBg":"#BF360C","clearBg":"#F57F17","confusionBg":"#4A148C","textColor":"#ccc","labelColor":"#e0dfdf"},"quadrant1Fill":"#1f2020","quadrant2Fill":"#242525","quadrant3Fill":"#292a2a","quadrant4Fill":"#2e2f2f","quadrant1TextFill":"#e0dfdf","quadrant2TextFill":"#dbdada","quadrant3TextFill":"#d6d5d5","quadrant4TextFill":"#d1d0d0","quadrantPointFill":"hsl(180, 1.5873015873%, NaN%)","quadrantPointTextFill":"#e0dfdf","quadrantXAxisTextFill":"#e0dfdf","quadrantYAxisTextFill":"#e0dfdf","quadrantInternalBorderStrokeFill":"#cccccc","quadrantExternalBorderStrokeFill":"#cccccc","quadrantTitleFill":"#e0dfdf","xyChart":{"backgroundColor":"#333","titleColor":"#e0dfdf","dataLabelColor":"#e0dfdf","legendTextColor":"#e0dfdf","xAxisTitleColor":"#e0dfdf","xAxisLabelColor":"#e0dfdf","xAxisTickColor":"#e0dfdf","xAxisLineColor":"#e0dfdf","yAxisTitleColor":"#e0dfdf","yAxisLabelColor":"#e0dfdf","yAxisTickColor":"#e0dfdf","yAxisLineColor":"#e0dfdf","plotColorPalette":"#3498db,#2ecc71,#e74c3c,#f1c40f,#bdc3c7,#ffffff,#34495e,#9b59b6,#1abc9c,#e67e22"},"packet":{"startByteColor":"#e0dfdf","endByteColor":"#e0dfdf","labelColor":"#e0dfdf","titleColor":"#e0dfdf","blockStrokeColor":"#e0dfdf","blockFillColor":"#333"},"radar":{"axisColor":"lightgrey","axisStrokeWidth":2,"axisLabelFontSize":12,"curveOpacity":0.5,"curveStrokeWidth":2,"graticuleColor":"#DEDEDE","graticuleStrokeWidth":1,"graticuleOpacity":0.3,"legendBoxSize":12,"legendFontSize":12},"wardleyEvolutionColor":"#ff6b6b","wardley":{"backgroundColor":"#333","axisColor":"lightgrey","axisTextColor":"#e0dfdf","gridColor":"lightgrey","componentFill":"#1f2020","componentStroke":"lightgrey","componentLabelColor":"#e0dfdf","linkStroke":"lightgrey","evolutionStroke":"#ff6b6b","annotationStroke":"lightgrey","annotationTextColor":"#e0dfdf","annotationFill":"#1f2020"},"classText":"#e0dfdf","requirementBackground":"#1f2020","requirementBorderColor":"#cccccc","requirementBorderSize":"1","requirementTextColor":"#e0dfdf","relationColor":"lightgrey","relationLabelBackground":"hsl(180, 1.5873015873%, 28.3529411765%)","relationLabelColor":"lightgrey","git0":"hsl(180, 1.5873015873%, 48.3529411765%)","git1":"hsl(321.6393442623, 65.5913978495%, 38.2352941176%)","git2":"hsl(194.4, 16.5562913907%, 49.6078431373%)","git3":"hsl(23.0769230769, 49.0566037736%, 40.7843137255%)","git4":"hsl(0, 83.3333333333%, 43.5294117647%)","git5":"hsl(289.1666666667, 100%, 24.1176470588%)","git6":"hsl(35.1315789474, 98.7012987013%, 40.1960784314%)","git7":"hsl(106.1538461538, 84.4155844156%, 35.0980392157%)","gitInv0":"rgb(133.6571428571, 129.7428571428, 129.7428571428)","gitInv1":"rgb(93.5483870969, 221.4516129033, 139.677419355)","gitInv2":"rgb(149.4437086091, 117.6092715231, 107.5562913906)","gitInv3":"rgb(99.9811320754, 162.7735849057, 202.0188679245)","gitInv4":"rgb(51.5000000001, 236.5, 236.5)","gitInv5":"rgb(154.2083333334, 255, 132.0000000001)","gitInv6":"rgb(51.331168831, 135.1948051946, 253.6688311688)","gitInv7":"rgb(206.1818181817, 89.948051948, 241.051948052)","gitBranchLabel0":"#2c2c2c","gitBranchLabel1":"lightgrey","gitBranchLabel2":"lightgrey","gitBranchLabel3":"#2c2c2c","gitBranchLabel4":"lightgrey","gitBranchLabel5":"lightgrey","gitBranchLabel6":"lightgrey","gitBranchLabel7":"lightgrey","tagLabelColor":"#e0dfdf","tagLabelBackground":"#1f2020","tagLabelBorder":"#cccccc","tagLabelFontSize":"10px","commitLabelColor":"rgb(183.8476190475, 181.5523809523, 181.5523809523)","commitLabelBackground":"hsl(180, 1.5873015873%, 28.3529411765%)","commitLabelFontSize":"10px","emUiFill":"#2d2d2d","emUiStroke":"#555","emProcessorFill":"hsl(296.1290322581, 20.2614379085%, 40%)","emProcessorStroke":"#8a6d8c","emReadModelFill":"hsl(98.6666666667, 33.3333333333%, 36.4705882353%)","emReadModelStroke":"#6d8c5c","emCommandFill":"hsl(218.6666666667, 33.3333333333%, 36.4705882353%)","emCommandStroke":"#5c6d8c","emEventFill":"hsl(32, 33.3333333333%, 36.4705882353%)","emEventStroke":"#8c755c","emSwimlaneBackgroundOdd":"hsl(0, 0%, 25%)","emSwimlaneBackgroundStroke":"hsl(0, 0%, 32%)","emArrowhead":"lightgrey","emRelationStroke":"lightgrey","attributeBackgroundColorOdd":"hsl(0, 0%, 32%)","attributeBackgroundColorEven":"hsl(0, 0%, 22%)"}"##;
    const FOREST_NONE: &str = r##"{"background":"white","primaryColor":"#cde498","secondaryColor":"#cdffb2","mainBkg":"#cde498","secondBkg":"#cdffb2","lineColor":"#000000","border1":"#13540c","border2":"#6eaa49","arrowheadColor":"green","fontFamily":"\"trebuchet ms\", verdana, arial, sans-serif","fontSize":"16px","tertiaryColor":"hsl(78.1578947368, 58.4615384615%, 84.5098039216%)","primaryBorderColor":"hsl(78.1578947368, 18.4615384615%, 64.5098039216%)","secondaryBorderColor":"hsl(98.961038961, 60%, 74.9019607843%)","tertiaryBorderColor":"hsl(78.1578947368, 18.4615384615%, 74.5098039216%)","primaryTextColor":"#321b67","secondaryTextColor":"#32004d","tertiaryTextColor":"#321b67","textColor":"#000000","THEME_COLOR_LIMIT":12,"radius":5,"strokeWidth":1,"nodeBkg":"#cde498","nodeBorder":"#13540c","clusterBkg":"#cdffb2","clusterBorder":"#6eaa49","defaultLinkColor":"#000000","titleColor":"#333","edgeLabelBackground":"#e8e8e8","actorBorder":"hsl(78.1578947368, 58.4615384615%, 54.5098039216%)","actorBkg":"#cde498","actorTextColor":"black","actorLineColor":"hsl(78.1578947368, 58.4615384615%, 54.5098039216%)","signalColor":"#333","signalTextColor":"#333","labelBoxBkgColor":"#cde498","labelBoxBorderColor":"#326932","labelTextColor":"black","loopTextColor":"black","noteBorderColor":"#6eaa49","noteBkgColor":"#fff5ad","noteTextColor":"black","activationBorderColor":"#666","activationBkgColor":"#f4f4f4","sequenceNumberColor":"white","sectionBkgColor":"#6eaa49","altSectionBkgColor":"white","sectionBkgColor2":"#6eaa49","excludeBkgColor":"#eeeeee","taskBorderColor":"#13540c","taskBkgColor":"#487e3a","taskTextLightColor":"white","taskTextColor":"white","taskTextDarkColor":"black","taskTextOutsideColor":"black","taskTextClickableColor":"#003163","activeTaskBorderColor":"#13540c","activeTaskBkgColor":"#cde498","gridColor":"lightgrey","doneTaskBkgColor":"lightgrey","doneTaskBorderColor":"grey","critBorderColor":"#ff8888","critBkgColor":"red","todayLineColor":"red","vertLineColor":"#00BFFF","personBorder":"hsl(78.1578947368, 18.4615384615%, 64.5098039216%)","personBkg":"#cde498","archEdgeColor":"#000000","archEdgeArrowColor":"#000000","archEdgeWidth":"3","archGroupBorderColor":"hsl(78.1578947368, 18.4615384615%, 64.5098039216%)","archGroupBorderWidth":"2px","noteFontWeight":"normal","fontWeight":"normal","labelColor":"black","errorBkgColor":"#552222","errorTextColor":"#552222","useGradient":true,"gradientStart":"hsl(78.1578947368, 18.4615384615%, 64.5098039216%)","gradientStop":"hsl(98.961038961, 60%, 74.9019607843%)","dropShadow":"drop-shadow( 1px 2px 2px rgba(185,185,185,0.5))","rectBkgColor":"hsl(78.1578947368, 58.4615384615%, 84.5098039216%)","cScale0":"hsl(78.1578947368, 58.4615384615%, 64.5098039216%)","cScale1":"hsl(98.961038961, 100%, 74.9019607843%)","cScale2":"hsl(78.1578947368, 58.4615384615%, 74.5098039216%)","cScale3":"hsl(108.1578947368, 58.4615384615%, 64.5098039216%)","cScale4":"hsl(138.1578947368, 58.4615384615%, 64.5098039216%)","cScale5":"hsl(168.1578947368, 58.4615384615%, 64.5098039216%)","cScale6":"hsl(198.1578947368, 58.4615384615%, 64.5098039216%)","cScale7":"hsl(228.1578947368, 58.4615384615%, 64.5098039216%)","cScale8":"hsl(288.1578947368, 58.4615384615%, 64.5098039216%)","cScale9":"hsl(348.1578947368, 58.4615384615%, 64.5098039216%)","cScale10":"hsl(18.1578947368, 58.4615384615%, 64.5098039216%)","cScale11":"hsl(48.1578947368, 58.4615384615%, 64.5098039216%)","cScalePeer1":"hsl(98.961038961, 100%, 39.9019607843%)","cScalePeer2":"hsl(78.1578947368, 58.4615384615%, 44.5098039216%)","cScalePeer0":"hsl(78.1578947368, 58.4615384615%, 39.5098039216%)","cScalePeer3":"hsl(108.1578947368, 58.4615384615%, 39.5098039216%)","cScalePeer4":"hsl(138.1578947368, 58.4615384615%, 39.5098039216%)","cScalePeer5":"hsl(168.1578947368, 58.4615384615%, 39.5098039216%)","cScalePeer6":"hsl(198.1578947368, 58.4615384615%, 39.5098039216%)","cScalePeer7":"hsl(228.1578947368, 58.4615384615%, 39.5098039216%)","cScalePeer8":"hsl(288.1578947368, 58.4615384615%, 39.5098039216%)","cScalePeer9":"hsl(348.1578947368, 58.4615384615%, 39.5098039216%)","cScalePeer10":"hsl(18.1578947368, 58.4615384615%, 39.5098039216%)","cScalePeer11":"hsl(48.1578947368, 58.4615384615%, 39.5098039216%)","cScaleInv0":"hsl(258.1578947368, 58.4615384615%, 64.5098039216%)","cScaleInv1":"hsl(278.961038961, 100%, 74.9019607843%)","cScaleInv2":"hsl(258.1578947368, 58.4615384615%, 74.5098039216%)","cScaleInv3":"hsl(288.1578947368, 58.4615384615%, 64.5098039216%)","cScaleInv4":"hsl(318.1578947368, 58.4615384615%, 64.5098039216%)","cScaleInv5":"hsl(348.1578947368, 58.4615384615%, 64.5098039216%)","cScaleInv6":"hsl(18.1578947368, 58.4615384615%, 64.5098039216%)","cScaleInv7":"hsl(48.1578947368, 58.4615384615%, 64.5098039216%)","cScaleInv8":"hsl(108.1578947368, 58.4615384615%, 64.5098039216%)","cScaleInv9":"hsl(168.1578947368, 58.4615384615%, 64.5098039216%)","cScaleInv10":"hsl(198.1578947368, 58.4615384615%, 64.5098039216%)","cScaleInv11":"hsl(228.1578947368, 58.4615384615%, 64.5098039216%)","scaleLabelColor":"black","cScaleLabel0":"black","cScaleLabel1":"black","cScaleLabel2":"black","cScaleLabel3":"black","cScaleLabel4":"black","cScaleLabel5":"black","cScaleLabel6":"black","cScaleLabel7":"black","cScaleLabel8":"black","cScaleLabel9":"black","cScaleLabel10":"black","cScaleLabel11":"black","surface0":"hsl(108.1578947368, 28.4615384615%, 69.5098039216%)","surfacePeer0":"hsl(108.1578947368, 28.4615384615%, 66.5098039216%)","surface1":"hsl(108.1578947368, 28.4615384615%, 64.5098039216%)","surfacePeer1":"hsl(108.1578947368, 28.4615384615%, 61.5098039216%)","surface2":"hsl(108.1578947368, 28.4615384615%, 59.5098039216%)","surfacePeer2":"hsl(108.1578947368, 28.4615384615%, 56.5098039216%)","surface3":"hsl(108.1578947368, 28.4615384615%, 54.5098039216%)","surfacePeer3":"hsl(108.1578947368, 28.4615384615%, 51.5098039216%)","surface4":"hsl(108.1578947368, 28.4615384615%, 49.5098039216%)","surfacePeer4":"hsl(108.1578947368, 28.4615384615%, 46.5098039216%)","flowContainerStroke":"hsl(98.961038961, 60%, 74.9019607843%)","rowOdd":"hsl(78.1578947368, 58.4615384615%, 100%)","rowEven":"hsl(78.1578947368, 58.4615384615%, 94.5098039216%)","transitionColor":"#000000","transitionLabelColor":"#000000","stateLabelColor":"#321b67","stateBkg":"#cde498","labelBackgroundColor":"#cde498","compositeBackground":"white","altBackground":"#f0f0f0","compositeTitleBackground":"#cde498","compositeBorder":"#13540c","innerEndBackground":"hsl(78.1578947368, 18.4615384615%, 64.5098039216%)","specialStateColor":"#000000","classText":"#321b67","fillType0":"#cde498","fillType1":"#cdffb2","fillType2":"hsl(142.1578947368, 58.4615384615%, 74.5098039216%)","fillType3":"hsl(162.961038961, 100%, 84.9019607843%)","fillType4":"hsl(14.1578947368, 58.4615384615%, 74.5098039216%)","fillType5":"hsl(34.961038961, 100%, 84.9019607843%)","fillType6":"hsl(206.1578947368, 58.4615384615%, 74.5098039216%)","fillType7":"hsl(226.961038961, 100%, 84.9019607843%)","pie1":"#cde498","pie2":"#cdffb2","pie3":"hsl(78.1578947368, 58.4615384615%, 84.5098039216%)","pie4":"hsl(78.1578947368, 58.4615384615%, 44.5098039216%)","pie5":"hsl(98.961038961, 100%, 54.9019607843%)","pie6":"hsl(118.1578947368, 58.4615384615%, 44.5098039216%)","pie7":"hsl(138.1578947368, 58.4615384615%, 64.5098039216%)","pie8":"hsl(18.1578947368, 58.4615384615%, 64.5098039216%)","pie9":"hsl(198.1578947368, 58.4615384615%, 74.5098039216%)","pie10":"hsl(138.1578947368, 58.4615384615%, 24.5098039216%)","pie11":"hsl(18.1578947368, 58.4615384615%, 24.5098039216%)","pie12":"hsl(198.1578947368, 58.4615384615%, 24.5098039216%)","pieTitleTextSize":"25px","pieTitleTextColor":"black","pieSectionTextSize":"17px","pieSectionTextColor":"#000000","pieLegendTextSize":"17px","pieLegendTextColor":"black","pieStrokeColor":"black","pieStrokeWidth":"2px","pieOuterStrokeWidth":"2px","pieOuterStrokeColor":"black","pieOpacity":"0.7","venn1":"hsl(78.1578947368, 58.4615384615%, 44.5098039216%)","venn2":"hsl(98.961038961, 100%, 54.9019607843%)","venn3":"hsl(78.1578947368, 58.4615384615%, 54.5098039216%)","venn4":"hsl(138.1578947368, 58.4615384615%, 44.5098039216%)","venn5":"hsl(18.1578947368, 58.4615384615%, 44.5098039216%)","venn6":"hsl(158.961038961, 100%, 54.9019607843%)","venn7":"hsl(198.1578947368, 58.4615384615%, 44.5098039216%)","venn8":"hsl(218.961038961, 100%, 54.9019607843%)","vennTitleTextColor":"#333","vennSetTextColor":"#000000","cynefin":{"domainFontSize":16,"itemFontSize":12,"boundaryColor":"#000000","boundaryWidth":2,"cliffColor":"#8B4513","cliffWidth":4,"arrowColor":"#000000","arrowWidth":2,"complexBg":"#C8E6C9","complicatedBg":"#DCEDC8","chaoticBg":"#FFE0B2","clearBg":"#FFF9C4","confusionBg":"#D7CCC8","textColor":"#000000","labelColor":"#321b67"},"quadrant1Fill":"#cde498","quadrant2Fill":"#d2e99d","quadrant3Fill":"#d7eea2","quadrant4Fill":"#dcf3a7","quadrant1TextFill":"#321b67","quadrant2TextFill":"#2d1662","quadrant3TextFill":"#28115d","quadrant4TextFill":"#230c58","quadrantPointFill":"hsl(78.1578947368, 58.4615384615%, NaN%)","quadrantPointTextFill":"#321b67","quadrantXAxisTextFill":"#321b67","quadrantYAxisTextFill":"#321b67","quadrantInternalBorderStrokeFill":"hsl(78.1578947368, 18.4615384615%, 64.5098039216%)","quadrantExternalBorderStrokeFill":"hsl(78.1578947368, 18.4615384615%, 64.5098039216%)","quadrantTitleFill":"#321b67","packet":{"startByteColor":"#321b67","endByteColor":"#321b67","labelColor":"#321b67","titleColor":"#321b67","blockStrokeColor":"#321b67","blockFillColor":"#cde498"},"radar":{"axisColor":"#000000","axisStrokeWidth":2,"axisLabelFontSize":12,"curveOpacity":0.5,"curveStrokeWidth":2,"graticuleColor":"#DEDEDE","graticuleStrokeWidth":1,"graticuleOpacity":0.3,"legendBoxSize":12,"legendFontSize":12},"wardleyEvolutionColor":"#dc3545","wardley":{"backgroundColor":"white","axisColor":"#000000","axisTextColor":"#321b67","gridColor":"lightgrey","componentFill":"white","componentStroke":"#000000","componentLabelColor":"#321b67","linkStroke":"#000000","evolutionStroke":"#dc3545","annotationStroke":"#000000","annotationTextColor":"#321b67","annotationFill":"white"},"xyChart":{"backgroundColor":"white","titleColor":"#321b67","dataLabelColor":"#321b67","legendTextColor":"#321b67","xAxisTitleColor":"#321b67","xAxisLabelColor":"#321b67","xAxisTickColor":"#321b67","xAxisLineColor":"#321b67","yAxisTitleColor":"#321b67","yAxisLabelColor":"#321b67","yAxisTickColor":"#321b67","yAxisLineColor":"#321b67","plotColorPalette":"#CDE498,#FF6B6B,#A0D2DB,#D7BDE2,#F0F0F0,#FFC3A0,#7FD8BE,#FF9A8B,#FAF3E0,#FFF176"},"requirementBackground":"#cde498","requirementBorderColor":"hsl(78.1578947368, 18.4615384615%, 64.5098039216%)","requirementBorderSize":"1","requirementTextColor":"#321b67","relationColor":"#000000","relationLabelBackground":"#e8e8e8","relationLabelColor":"black","git0":"hsl(78.1578947368, 58.4615384615%, 49.5098039216%)","git1":"hsl(98.961038961, 100%, 59.9019607843%)","git2":"hsl(78.1578947368, 58.4615384615%, 59.5098039216%)","git3":"hsl(48.1578947368, 58.4615384615%, 49.5098039216%)","git4":"hsl(18.1578947368, 58.4615384615%, 49.5098039216%)","git5":"hsl(-11.8421052632, 58.4615384615%, 49.5098039216%)","git6":"hsl(138.1578947368, 58.4615384615%, 49.5098039216%)","git7":"hsl(198.1578947368, 58.4615384615%, 49.5098039216%)","gitInv0":"rgb(99.6153846152, 54.9423076922, 202.5576923076)","gitInv1":"rgb(132.7922077921, 0, 204.5000000001)","gitInv2":"rgb(79.4230769229, 42.8884615385, 163.6115384614)","gitInv3":"rgb(54.9423076922, 84.0769230769, 202.5576923076)","gitInv4":"rgb(54.9423076922, 157.8846153846, 202.5576923076)","gitInv5":"rgb(54.9423076922, 202.5576923076, 173.4230769229)","gitInv6":"rgb(202.5576923076, 54.9423076922, 157.8846153846)","gitInv7":"rgb(202.5576923076, 99.6153846152, 54.9423076922)","gitBranchLabel0":"#ffffff","gitBranchLabel1":"black","gitBranchLabel2":"black","gitBranchLabel3":"#ffffff","gitBranchLabel4":"black","gitBranchLabel5":"black","gitBranchLabel6":"black","gitBranchLabel7":"black","tagLabelColor":"#321b67","tagLabelBackground":"#cde498","tagLabelBorder":"hsl(78.1578947368, 18.4615384615%, 64.5098039216%)","tagLabelFontSize":"10px","commitLabelColor":"#32004d","commitLabelBackground":"#cdffb2","commitLabelFontSize":"10px","emUiFill":"white","emUiStroke":"#dbdada","emProcessorFill":"#edb3f6","emProcessorStroke":"#b88cbf","emReadModelFill":"#d3f1a2","emReadModelStroke":"#a3b732","emCommandFill":"#bcd6fe","emCommandStroke":"#679ac3","emEventFill":"#ffb778","emEventStroke":"#c19a0f","emSwimlaneBackgroundOdd":"rgb(250,250,250)","emSwimlaneBackgroundStroke":"rgb(240,240,240)","emArrowhead":"#000000","emRelationStroke":"#000000","attributeBackgroundColorOdd":"#ffffff","attributeBackgroundColorEven":"#f2f2f2"}"##;
    const FOREST_COLORS: &str = r##"{"background":"white","primaryColor":"#abcdef","secondaryColor":"#123456","mainBkg":"#cde498","secondBkg":"#cdffb2","lineColor":"#000000","border1":"#13540c","border2":"#6eaa49","arrowheadColor":"green","fontFamily":"\"trebuchet ms\", verdana, arial, sans-serif","fontSize":"16px","tertiaryColor":"hsl(78.1578947368, 58.4615384615%, 84.5098039216%)","primaryBorderColor":"hsl(78.1578947368, 18.4615384615%, 64.5098039216%)","secondaryBorderColor":"hsl(98.961038961, 60%, 74.9019607843%)","tertiaryBorderColor":"hsl(78.1578947368, 18.4615384615%, 74.5098039216%)","primaryTextColor":"#321b67","secondaryTextColor":"#32004d","tertiaryTextColor":"#321b67","textColor":"#000000","THEME_COLOR_LIMIT":12,"radius":5,"strokeWidth":1,"nodeBkg":"#cde498","nodeBorder":"#13540c","clusterBkg":"#cdffb2","clusterBorder":"#6eaa49","defaultLinkColor":"#000000","titleColor":"#333","edgeLabelBackground":"#e8e8e8","actorBorder":"hsl(78.1578947368, 58.4615384615%, 54.5098039216%)","actorBkg":"#cde498","actorTextColor":"black","actorLineColor":"hsl(78.1578947368, 58.4615384615%, 54.5098039216%)","signalColor":"#333","signalTextColor":"#333","labelBoxBkgColor":"#cde498","labelBoxBorderColor":"#326932","labelTextColor":"black","loopTextColor":"black","noteBorderColor":"#6eaa49","noteBkgColor":"#fff5ad","noteTextColor":"black","activationBorderColor":"#666","activationBkgColor":"#f4f4f4","sequenceNumberColor":"white","sectionBkgColor":"#6eaa49","altSectionBkgColor":"white","sectionBkgColor2":"#6eaa49","excludeBkgColor":"#eeeeee","taskBorderColor":"#13540c","taskBkgColor":"#487e3a","taskTextLightColor":"white","taskTextColor":"white","taskTextDarkColor":"black","taskTextOutsideColor":"black","taskTextClickableColor":"#003163","activeTaskBorderColor":"#13540c","activeTaskBkgColor":"#cde498","gridColor":"lightgrey","doneTaskBkgColor":"lightgrey","doneTaskBorderColor":"grey","critBorderColor":"#ff8888","critBkgColor":"red","todayLineColor":"red","vertLineColor":"#00BFFF","personBorder":"hsl(78.1578947368, 18.4615384615%, 64.5098039216%)","personBkg":"#cde498","archEdgeColor":"#000000","archEdgeArrowColor":"#000000","archEdgeWidth":"3","archGroupBorderColor":"hsl(78.1578947368, 18.4615384615%, 64.5098039216%)","archGroupBorderWidth":"2px","noteFontWeight":"normal","fontWeight":"normal","labelColor":"black","errorBkgColor":"#552222","errorTextColor":"#552222","useGradient":true,"gradientStart":"hsl(78.1578947368, 18.4615384615%, 64.5098039216%)","gradientStop":"hsl(98.961038961, 60%, 74.9019607843%)","dropShadow":"drop-shadow( 1px 2px 2px rgba(185,185,185,0.5))","darkMode":true,"rectBkgColor":"hsl(78.1578947368, 58.4615384615%, 84.5098039216%)","cScale0":"hsl(210, 68%, 70.3921568627%)","cScale1":"hsl(210, 65.3846153846%, 10.3921568627%)","cScale2":"hsl(78.1578947368, 58.4615384615%, 74.5098039216%)","cScale3":"hsl(240, 68%, 70.3921568627%)","cScale4":"hsl(270, 68%, 70.3921568627%)","cScale5":"hsl(300, 68%, 70.3921568627%)","cScale6":"hsl(330, 68%, 70.3921568627%)","cScale7":"hsl(0, 68%, 70.3921568627%)","cScale8":"hsl(60, 68%, 70.3921568627%)","cScale9":"hsl(120, 68%, 70.3921568627%)","cScale10":"hsl(150, 68%, 70.3921568627%)","cScale11":"hsl(180, 68%, 70.3921568627%)","cScalePeer1":"hsl(210, 65.3846153846%, 0%)","cScalePeer2":"hsl(78.1578947368, 58.4615384615%, 44.5098039216%)","cScalePeer0":"hsl(210, 68%, 45.3921568627%)","cScalePeer3":"hsl(240, 68%, 45.3921568627%)","cScalePeer4":"hsl(270, 68%, 45.3921568627%)","cScalePeer5":"hsl(300, 68%, 45.3921568627%)","cScalePeer6":"hsl(330, 68%, 45.3921568627%)","cScalePeer7":"hsl(0, 68%, 45.3921568627%)","cScalePeer8":"hsl(60, 68%, 45.3921568627%)","cScalePeer9":"hsl(120, 68%, 45.3921568627%)","cScalePeer10":"hsl(150, 68%, 45.3921568627%)","cScalePeer11":"hsl(180, 68%, 45.3921568627%)","cScaleInv0":"hsl(30, 68%, 70.3921568627%)","cScaleInv1":"hsl(30, 65.3846153846%, 10.3921568627%)","cScaleInv2":"hsl(258.1578947368, 58.4615384615%, 74.5098039216%)","cScaleInv3":"hsl(60, 68%, 70.3921568627%)","cScaleInv4":"hsl(90, 68%, 70.3921568627%)","cScaleInv5":"hsl(120, 68%, 70.3921568627%)","cScaleInv6":"hsl(150, 68%, 70.3921568627%)","cScaleInv7":"hsl(180, 68%, 70.3921568627%)","cScaleInv8":"hsl(240, 68%, 70.3921568627%)","cScaleInv9":"hsl(300, 68%, 70.3921568627%)","cScaleInv10":"hsl(330, 68%, 70.3921568627%)","cScaleInv11":"hsl(0, 68%, 70.3921568627%)","scaleLabelColor":"black","cScaleLabel0":"black","cScaleLabel1":"black","cScaleLabel2":"black","cScaleLabel3":"black","cScaleLabel4":"black","cScaleLabel5":"black","cScaleLabel6":"black","cScaleLabel7":"black","cScaleLabel8":"black","cScaleLabel9":"black","cScaleLabel10":"black","cScaleLabel11":"black","surface0":"hsl(108.1578947368, 28.4615384615%, 69.5098039216%)","surfacePeer0":"hsl(108.1578947368, 28.4615384615%, 66.5098039216%)","surface1":"hsl(108.1578947368, 28.4615384615%, 64.5098039216%)","surfacePeer1":"hsl(108.1578947368, 28.4615384615%, 61.5098039216%)","surface2":"hsl(108.1578947368, 28.4615384615%, 59.5098039216%)","surfacePeer2":"hsl(108.1578947368, 28.4615384615%, 56.5098039216%)","surface3":"hsl(108.1578947368, 28.4615384615%, 54.5098039216%)","surfacePeer3":"hsl(108.1578947368, 28.4615384615%, 51.5098039216%)","surface4":"hsl(108.1578947368, 28.4615384615%, 49.5098039216%)","surfacePeer4":"hsl(108.1578947368, 28.4615384615%, 46.5098039216%)","flowContainerStroke":"hsl(98.961038961, 60%, 74.9019607843%)","rowOdd":"hsl(78.1578947368, 58.4615384615%, 100%)","rowEven":"hsl(78.1578947368, 58.4615384615%, 94.5098039216%)","transitionColor":"#000000","transitionLabelColor":"#000000","stateLabelColor":"#321b67","stateBkg":"#cde498","labelBackgroundColor":"#cde498","compositeBackground":"white","altBackground":"#f0f0f0","compositeTitleBackground":"#cde498","compositeBorder":"#13540c","innerEndBackground":"hsl(78.1578947368, 18.4615384615%, 64.5098039216%)","specialStateColor":"#000000","classText":"#321b67","fillType0":"#abcdef","fillType1":"#123456","fillType2":"hsl(274, 68%, 80.3921568627%)","fillType3":"hsl(274, 65.3846153846%, 20.3921568627%)","fillType4":"hsl(146, 68%, 80.3921568627%)","fillType5":"hsl(146, 65.3846153846%, 20.3921568627%)","fillType6":"hsl(338, 68%, 80.3921568627%)","fillType7":"hsl(338, 65.3846153846%, 20.3921568627%)","pie1":"#abcdef","pie2":"#123456","pie3":"hsl(78.1578947368, 58.4615384615%, 84.5098039216%)","pie4":"hsl(210, 68%, 50.3921568627%)","pie5":"hsl(210, 65.3846153846%, 0%)","pie6":"hsl(118.1578947368, 58.4615384615%, 44.5098039216%)","pie7":"hsl(270, 68%, 70.3921568627%)","pie8":"hsl(150, 68%, 70.3921568627%)","pie9":"hsl(330, 68%, 80.3921568627%)","pie10":"hsl(270, 68%, 30.3921568627%)","pie11":"hsl(150, 68%, 30.3921568627%)","pie12":"hsl(330, 68%, 30.3921568627%)","pieTitleTextSize":"25px","pieTitleTextColor":"black","pieSectionTextSize":"17px","pieSectionTextColor":"#000000","pieLegendTextSize":"17px","pieLegendTextColor":"black","pieStrokeColor":"black","pieStrokeWidth":"2px","pieOuterStrokeWidth":"2px","pieOuterStrokeColor":"black","pieOpacity":"0.7","venn1":"hsl(210, 68%, 50.3921568627%)","venn2":"hsl(210, 65.3846153846%, 0%)","venn3":"hsl(78.1578947368, 58.4615384615%, 54.5098039216%)","venn4":"hsl(270, 68%, 50.3921568627%)","venn5":"hsl(150, 68%, 50.3921568627%)","venn6":"hsl(270, 65.3846153846%, 0%)","venn7":"hsl(330, 68%, 50.3921568627%)","venn8":"hsl(330, 65.3846153846%, 0%)","vennTitleTextColor":"#333","vennSetTextColor":"#000000","cynefin":{"domainFontSize":16,"itemFontSize":12,"boundaryColor":"#000000","boundaryWidth":2,"cliffColor":"#8B4513","cliffWidth":4,"arrowColor":"#000000","arrowWidth":2,"complexBg":"#C8E6C9","complicatedBg":"#DCEDC8","chaoticBg":"#FFE0B2","clearBg":"#FFF9C4","confusionBg":"#D7CCC8","textColor":"#000000","labelColor":"#321b67"},"quadrant1Fill":"#abcdef","quadrant2Fill":"#b0d2f4","quadrant3Fill":"#b5d7f9","quadrant4Fill":"#badcfe","quadrant1TextFill":"#321b67","quadrant2TextFill":"#2d1662","quadrant3TextFill":"#28115d","quadrant4TextFill":"#230c58","quadrantPointFill":"hsl(210, 68%, NaN%)","quadrantPointTextFill":"#321b67","quadrantXAxisTextFill":"#321b67","quadrantYAxisTextFill":"#321b67","quadrantInternalBorderStrokeFill":"hsl(78.1578947368, 18.4615384615%, 64.5098039216%)","quadrantExternalBorderStrokeFill":"hsl(78.1578947368, 18.4615384615%, 64.5098039216%)","quadrantTitleFill":"#321b67","packet":{"startByteColor":"#321b67","endByteColor":"#321b67","labelColor":"#321b67","titleColor":"#321b67","blockStrokeColor":"#321b67","blockFillColor":"#cde498"},"radar":{"axisColor":"#000000","axisStrokeWidth":2,"axisLabelFontSize":12,"curveOpacity":0.5,"curveStrokeWidth":2,"graticuleColor":"#DEDEDE","graticuleStrokeWidth":1,"graticuleOpacity":0.3,"legendBoxSize":12,"legendFontSize":12},"wardleyEvolutionColor":"#dc3545","wardley":{"backgroundColor":"white","axisColor":"#000000","axisTextColor":"#321b67","gridColor":"lightgrey","componentFill":"white","componentStroke":"#000000","componentLabelColor":"#321b67","linkStroke":"#000000","evolutionStroke":"#dc3545","annotationStroke":"#000000","annotationTextColor":"#321b67","annotationFill":"white"},"xyChart":{"backgroundColor":"white","titleColor":"#321b67","dataLabelColor":"#321b67","legendTextColor":"#321b67","xAxisTitleColor":"#321b67","xAxisLabelColor":"#321b67","xAxisTickColor":"#321b67","xAxisLineColor":"#321b67","yAxisTitleColor":"#321b67","yAxisLabelColor":"#321b67","yAxisTickColor":"#321b67","yAxisLineColor":"#321b67","plotColorPalette":"#CDE498,#FF6B6B,#A0D2DB,#D7BDE2,#F0F0F0,#FFC3A0,#7FD8BE,#FF9A8B,#FAF3E0,#FFF176"},"requirementBackground":"#abcdef","requirementBorderColor":"hsl(78.1578947368, 18.4615384615%, 64.5098039216%)","requirementBorderSize":"1","requirementTextColor":"#321b67","relationColor":"#000000","relationLabelBackground":"#e8e8e8","relationLabelColor":"black","git0":"hsl(210, 68%, 100%)","git1":"hsl(210, 65.3846153846%, 45.3921568627%)","git2":"hsl(78.1578947368, 58.4615384615%, 100%)","git3":"hsl(180, 68%, 100%)","git4":"hsl(150, 68%, 100%)","git5":"hsl(120, 68%, 100%)","git6":"hsl(270, 68%, 100%)","git7":"hsl(330, 68%, 100%)","gitInv0":"#000000","gitInv1":"rgb(214.9326923077, 139.2500000001, 63.5673076925)","gitInv2":"#000000","gitInv3":"#000000","gitInv4":"#000000","gitInv5":"#000000","gitInv6":"#000000","gitInv7":"#000000","gitBranchLabel0":"#ffffff","gitBranchLabel1":"black","gitBranchLabel2":"black","gitBranchLabel3":"#ffffff","gitBranchLabel4":"black","gitBranchLabel5":"black","gitBranchLabel6":"black","gitBranchLabel7":"black","tagLabelColor":"#321b67","tagLabelBackground":"#abcdef","tagLabelBorder":"hsl(78.1578947368, 18.4615384615%, 64.5098039216%)","tagLabelFontSize":"10px","commitLabelColor":"#32004d","commitLabelBackground":"#123456","commitLabelFontSize":"10px","emUiFill":"white","emUiStroke":"#dbdada","emProcessorFill":"#edb3f6","emProcessorStroke":"#b88cbf","emReadModelFill":"#d3f1a2","emReadModelStroke":"#a3b732","emCommandFill":"#bcd6fe","emCommandStroke":"#679ac3","emEventFill":"#ffb778","emEventStroke":"#c19a0f","emSwimlaneBackgroundOdd":"rgb(250,250,250)","emSwimlaneBackgroundStroke":"rgb(240,240,240)","emArrowhead":"#000000","emRelationStroke":"#000000","attributeBackgroundColorOdd":"#ffffff","attributeBackgroundColorEven":"#f2f2f2"}"##;
    const FOREST_NESTED: &str = r##"{"background":"white","primaryColor":"#cde498","secondaryColor":"#cdffb2","mainBkg":"#cde498","secondBkg":"#cdffb2","lineColor":"#000000","border1":"#13540c","border2":"#6eaa49","arrowheadColor":"green","fontFamily":"\"trebuchet ms\", verdana, arial, sans-serif","fontSize":"16px","tertiaryColor":"hsl(78.1578947368, 58.4615384615%, 84.5098039216%)","primaryBorderColor":"hsl(78.1578947368, 18.4615384615%, 64.5098039216%)","secondaryBorderColor":"hsl(98.961038961, 60%, 74.9019607843%)","tertiaryBorderColor":"hsl(78.1578947368, 18.4615384615%, 74.5098039216%)","primaryTextColor":"#321b67","secondaryTextColor":"#32004d","tertiaryTextColor":"#321b67","textColor":"#000000","THEME_COLOR_LIMIT":12,"radius":5,"strokeWidth":1,"nodeBkg":"#cde498","nodeBorder":"#13540c","clusterBkg":"#cdffb2","clusterBorder":"#6eaa49","defaultLinkColor":"#000000","titleColor":"#333","edgeLabelBackground":"#e8e8e8","actorBorder":"hsl(78.1578947368, 58.4615384615%, 54.5098039216%)","actorBkg":"#cde498","actorTextColor":"black","actorLineColor":"hsl(78.1578947368, 58.4615384615%, 54.5098039216%)","signalColor":"#333","signalTextColor":"#333","labelBoxBkgColor":"#cde498","labelBoxBorderColor":"#326932","labelTextColor":"black","loopTextColor":"black","noteBorderColor":"#6eaa49","noteBkgColor":"#fff5ad","noteTextColor":"black","activationBorderColor":"#666","activationBkgColor":"#f4f4f4","sequenceNumberColor":"white","sectionBkgColor":"#6eaa49","altSectionBkgColor":"white","sectionBkgColor2":"#6eaa49","excludeBkgColor":"#eeeeee","taskBorderColor":"#13540c","taskBkgColor":"#487e3a","taskTextLightColor":"white","taskTextColor":"white","taskTextDarkColor":"black","taskTextOutsideColor":"black","taskTextClickableColor":"#003163","activeTaskBorderColor":"#13540c","activeTaskBkgColor":"#cde498","gridColor":"lightgrey","doneTaskBkgColor":"lightgrey","doneTaskBorderColor":"grey","critBorderColor":"#ff8888","critBkgColor":"red","todayLineColor":"red","vertLineColor":"#00BFFF","personBorder":"hsl(78.1578947368, 18.4615384615%, 64.5098039216%)","personBkg":"#cde498","archEdgeColor":"#000000","archEdgeArrowColor":"#000000","archEdgeWidth":"3","archGroupBorderColor":"hsl(78.1578947368, 18.4615384615%, 64.5098039216%)","archGroupBorderWidth":"2px","noteFontWeight":"normal","fontWeight":"normal","labelColor":"black","errorBkgColor":"#552222","errorTextColor":"#552222","useGradient":true,"gradientStart":"hsl(78.1578947368, 18.4615384615%, 64.5098039216%)","gradientStop":"hsl(98.961038961, 60%, 74.9019607843%)","dropShadow":"drop-shadow( 1px 2px 2px rgba(185,185,185,0.5))","xyChart":{"backgroundColor":"white","titleColor":"#ff0000","dataLabelColor":"#321b67","legendTextColor":"#321b67","xAxisTitleColor":"#321b67","xAxisLabelColor":"#321b67","xAxisTickColor":"#321b67","xAxisLineColor":"#321b67","yAxisTitleColor":"#321b67","yAxisLabelColor":"#321b67","yAxisTickColor":"#321b67","yAxisLineColor":"#321b67","plotColorPalette":"#CDE498,#FF6B6B,#A0D2DB,#D7BDE2,#F0F0F0,#FFC3A0,#7FD8BE,#FF9A8B,#FAF3E0,#FFF176"},"rectBkgColor":"hsl(78.1578947368, 58.4615384615%, 84.5098039216%)","cScale0":"hsl(78.1578947368, 58.4615384615%, 64.5098039216%)","cScale1":"hsl(98.961038961, 100%, 74.9019607843%)","cScale2":"hsl(78.1578947368, 58.4615384615%, 74.5098039216%)","cScale3":"hsl(108.1578947368, 58.4615384615%, 64.5098039216%)","cScale4":"hsl(138.1578947368, 58.4615384615%, 64.5098039216%)","cScale5":"hsl(168.1578947368, 58.4615384615%, 64.5098039216%)","cScale6":"hsl(198.1578947368, 58.4615384615%, 64.5098039216%)","cScale7":"hsl(228.1578947368, 58.4615384615%, 64.5098039216%)","cScale8":"hsl(288.1578947368, 58.4615384615%, 64.5098039216%)","cScale9":"hsl(348.1578947368, 58.4615384615%, 64.5098039216%)","cScale10":"hsl(18.1578947368, 58.4615384615%, 64.5098039216%)","cScale11":"hsl(48.1578947368, 58.4615384615%, 64.5098039216%)","cScalePeer1":"hsl(98.961038961, 100%, 39.9019607843%)","cScalePeer2":"hsl(78.1578947368, 58.4615384615%, 44.5098039216%)","cScalePeer0":"hsl(78.1578947368, 58.4615384615%, 39.5098039216%)","cScalePeer3":"hsl(108.1578947368, 58.4615384615%, 39.5098039216%)","cScalePeer4":"hsl(138.1578947368, 58.4615384615%, 39.5098039216%)","cScalePeer5":"hsl(168.1578947368, 58.4615384615%, 39.5098039216%)","cScalePeer6":"hsl(198.1578947368, 58.4615384615%, 39.5098039216%)","cScalePeer7":"hsl(228.1578947368, 58.4615384615%, 39.5098039216%)","cScalePeer8":"hsl(288.1578947368, 58.4615384615%, 39.5098039216%)","cScalePeer9":"hsl(348.1578947368, 58.4615384615%, 39.5098039216%)","cScalePeer10":"hsl(18.1578947368, 58.4615384615%, 39.5098039216%)","cScalePeer11":"hsl(48.1578947368, 58.4615384615%, 39.5098039216%)","cScaleInv0":"hsl(258.1578947368, 58.4615384615%, 64.5098039216%)","cScaleInv1":"hsl(278.961038961, 100%, 74.9019607843%)","cScaleInv2":"hsl(258.1578947368, 58.4615384615%, 74.5098039216%)","cScaleInv3":"hsl(288.1578947368, 58.4615384615%, 64.5098039216%)","cScaleInv4":"hsl(318.1578947368, 58.4615384615%, 64.5098039216%)","cScaleInv5":"hsl(348.1578947368, 58.4615384615%, 64.5098039216%)","cScaleInv6":"hsl(18.1578947368, 58.4615384615%, 64.5098039216%)","cScaleInv7":"hsl(48.1578947368, 58.4615384615%, 64.5098039216%)","cScaleInv8":"hsl(108.1578947368, 58.4615384615%, 64.5098039216%)","cScaleInv9":"hsl(168.1578947368, 58.4615384615%, 64.5098039216%)","cScaleInv10":"hsl(198.1578947368, 58.4615384615%, 64.5098039216%)","cScaleInv11":"hsl(228.1578947368, 58.4615384615%, 64.5098039216%)","scaleLabelColor":"black","cScaleLabel0":"black","cScaleLabel1":"black","cScaleLabel2":"black","cScaleLabel3":"black","cScaleLabel4":"black","cScaleLabel5":"black","cScaleLabel6":"black","cScaleLabel7":"black","cScaleLabel8":"black","cScaleLabel9":"black","cScaleLabel10":"black","cScaleLabel11":"black","surface0":"hsl(108.1578947368, 28.4615384615%, 69.5098039216%)","surfacePeer0":"hsl(108.1578947368, 28.4615384615%, 66.5098039216%)","surface1":"hsl(108.1578947368, 28.4615384615%, 64.5098039216%)","surfacePeer1":"hsl(108.1578947368, 28.4615384615%, 61.5098039216%)","surface2":"hsl(108.1578947368, 28.4615384615%, 59.5098039216%)","surfacePeer2":"hsl(108.1578947368, 28.4615384615%, 56.5098039216%)","surface3":"hsl(108.1578947368, 28.4615384615%, 54.5098039216%)","surfacePeer3":"hsl(108.1578947368, 28.4615384615%, 51.5098039216%)","surface4":"hsl(108.1578947368, 28.4615384615%, 49.5098039216%)","surfacePeer4":"hsl(108.1578947368, 28.4615384615%, 46.5098039216%)","flowContainerStroke":"hsl(98.961038961, 60%, 74.9019607843%)","rowOdd":"hsl(78.1578947368, 58.4615384615%, 100%)","rowEven":"hsl(78.1578947368, 58.4615384615%, 94.5098039216%)","transitionColor":"#000000","transitionLabelColor":"#000000","stateLabelColor":"#321b67","stateBkg":"#cde498","labelBackgroundColor":"#cde498","compositeBackground":"white","altBackground":"#f0f0f0","compositeTitleBackground":"#cde498","compositeBorder":"#13540c","innerEndBackground":"hsl(78.1578947368, 18.4615384615%, 64.5098039216%)","specialStateColor":"#000000","classText":"#321b67","fillType0":"#cde498","fillType1":"#cdffb2","fillType2":"hsl(142.1578947368, 58.4615384615%, 74.5098039216%)","fillType3":"hsl(162.961038961, 100%, 84.9019607843%)","fillType4":"hsl(14.1578947368, 58.4615384615%, 74.5098039216%)","fillType5":"hsl(34.961038961, 100%, 84.9019607843%)","fillType6":"hsl(206.1578947368, 58.4615384615%, 74.5098039216%)","fillType7":"hsl(226.961038961, 100%, 84.9019607843%)","pie1":"#cde498","pie2":"#cdffb2","pie3":"hsl(78.1578947368, 58.4615384615%, 84.5098039216%)","pie4":"hsl(78.1578947368, 58.4615384615%, 44.5098039216%)","pie5":"hsl(98.961038961, 100%, 54.9019607843%)","pie6":"hsl(118.1578947368, 58.4615384615%, 44.5098039216%)","pie7":"hsl(138.1578947368, 58.4615384615%, 64.5098039216%)","pie8":"hsl(18.1578947368, 58.4615384615%, 64.5098039216%)","pie9":"hsl(198.1578947368, 58.4615384615%, 74.5098039216%)","pie10":"hsl(138.1578947368, 58.4615384615%, 24.5098039216%)","pie11":"hsl(18.1578947368, 58.4615384615%, 24.5098039216%)","pie12":"hsl(198.1578947368, 58.4615384615%, 24.5098039216%)","pieTitleTextSize":"25px","pieTitleTextColor":"black","pieSectionTextSize":"17px","pieSectionTextColor":"#000000","pieLegendTextSize":"17px","pieLegendTextColor":"black","pieStrokeColor":"black","pieStrokeWidth":"2px","pieOuterStrokeWidth":"2px","pieOuterStrokeColor":"black","pieOpacity":"0.7","venn1":"hsl(78.1578947368, 58.4615384615%, 44.5098039216%)","venn2":"hsl(98.961038961, 100%, 54.9019607843%)","venn3":"hsl(78.1578947368, 58.4615384615%, 54.5098039216%)","venn4":"hsl(138.1578947368, 58.4615384615%, 44.5098039216%)","venn5":"hsl(18.1578947368, 58.4615384615%, 44.5098039216%)","venn6":"hsl(158.961038961, 100%, 54.9019607843%)","venn7":"hsl(198.1578947368, 58.4615384615%, 44.5098039216%)","venn8":"hsl(218.961038961, 100%, 54.9019607843%)","vennTitleTextColor":"#333","vennSetTextColor":"#000000","cynefin":{"domainFontSize":16,"itemFontSize":12,"boundaryColor":"#000000","boundaryWidth":2,"cliffColor":"#8B4513","cliffWidth":4,"arrowColor":"#000000","arrowWidth":2,"complexBg":"#C8E6C9","complicatedBg":"#DCEDC8","chaoticBg":"#FFE0B2","clearBg":"#FFF9C4","confusionBg":"#D7CCC8","textColor":"#000000","labelColor":"#321b67"},"quadrant1Fill":"#cde498","quadrant2Fill":"#d2e99d","quadrant3Fill":"#d7eea2","quadrant4Fill":"#dcf3a7","quadrant1TextFill":"#321b67","quadrant2TextFill":"#2d1662","quadrant3TextFill":"#28115d","quadrant4TextFill":"#230c58","quadrantPointFill":"hsl(78.1578947368, 58.4615384615%, NaN%)","quadrantPointTextFill":"#321b67","quadrantXAxisTextFill":"#321b67","quadrantYAxisTextFill":"#321b67","quadrantInternalBorderStrokeFill":"hsl(78.1578947368, 18.4615384615%, 64.5098039216%)","quadrantExternalBorderStrokeFill":"hsl(78.1578947368, 18.4615384615%, 64.5098039216%)","quadrantTitleFill":"#321b67","packet":{"startByteColor":"#321b67","endByteColor":"#321b67","labelColor":"#321b67","titleColor":"#321b67","blockStrokeColor":"#321b67","blockFillColor":"#cde498"},"radar":{"axisColor":"#000000","axisStrokeWidth":2,"axisLabelFontSize":12,"curveOpacity":0.5,"curveStrokeWidth":2,"graticuleColor":"#DEDEDE","graticuleStrokeWidth":1,"graticuleOpacity":0.3,"legendBoxSize":12,"legendFontSize":12},"wardleyEvolutionColor":"#dc3545","wardley":{"backgroundColor":"white","axisColor":"#000000","axisTextColor":"#321b67","gridColor":"lightgrey","componentFill":"white","componentStroke":"#000000","componentLabelColor":"#321b67","linkStroke":"#000000","evolutionStroke":"#dc3545","annotationStroke":"#000000","annotationTextColor":"#321b67","annotationFill":"white"},"requirementBackground":"#cde498","requirementBorderColor":"hsl(78.1578947368, 18.4615384615%, 64.5098039216%)","requirementBorderSize":"1","requirementTextColor":"#321b67","relationColor":"#000000","relationLabelBackground":"#e8e8e8","relationLabelColor":"black","git0":"hsl(78.1578947368, 58.4615384615%, 49.5098039216%)","git1":"hsl(98.961038961, 100%, 59.9019607843%)","git2":"hsl(78.1578947368, 58.4615384615%, 59.5098039216%)","git3":"hsl(48.1578947368, 58.4615384615%, 49.5098039216%)","git4":"hsl(18.1578947368, 58.4615384615%, 49.5098039216%)","git5":"hsl(-11.8421052632, 58.4615384615%, 49.5098039216%)","git6":"hsl(138.1578947368, 58.4615384615%, 49.5098039216%)","git7":"hsl(198.1578947368, 58.4615384615%, 49.5098039216%)","gitInv0":"rgb(99.6153846152, 54.9423076922, 202.5576923076)","gitInv1":"rgb(132.7922077921, 0, 204.5000000001)","gitInv2":"rgb(79.4230769229, 42.8884615385, 163.6115384614)","gitInv3":"rgb(54.9423076922, 84.0769230769, 202.5576923076)","gitInv4":"rgb(54.9423076922, 157.8846153846, 202.5576923076)","gitInv5":"rgb(54.9423076922, 202.5576923076, 173.4230769229)","gitInv6":"rgb(202.5576923076, 54.9423076922, 157.8846153846)","gitInv7":"rgb(202.5576923076, 99.6153846152, 54.9423076922)","gitBranchLabel0":"#ffffff","gitBranchLabel1":"black","gitBranchLabel2":"black","gitBranchLabel3":"#ffffff","gitBranchLabel4":"black","gitBranchLabel5":"black","gitBranchLabel6":"black","gitBranchLabel7":"black","tagLabelColor":"#321b67","tagLabelBackground":"#cde498","tagLabelBorder":"hsl(78.1578947368, 18.4615384615%, 64.5098039216%)","tagLabelFontSize":"10px","commitLabelColor":"#32004d","commitLabelBackground":"#cdffb2","commitLabelFontSize":"10px","emUiFill":"white","emUiStroke":"#dbdada","emProcessorFill":"#edb3f6","emProcessorStroke":"#b88cbf","emReadModelFill":"#d3f1a2","emReadModelStroke":"#a3b732","emCommandFill":"#bcd6fe","emCommandStroke":"#679ac3","emEventFill":"#ffb778","emEventStroke":"#c19a0f","emSwimlaneBackgroundOdd":"rgb(250,250,250)","emSwimlaneBackgroundStroke":"rgb(240,240,240)","emArrowhead":"#000000","emRelationStroke":"#000000","attributeBackgroundColorOdd":"#ffffff","attributeBackgroundColorEven":"#f2f2f2"}"##;
    const FOREST_BORDER: &str = r##"{"background":"white","primaryColor":"#cde498","secondaryColor":"#cdffb2","mainBkg":"#cde498","secondBkg":"#cdffb2","lineColor":"#000000","border1":"#13540c","border2":"#6eaa49","arrowheadColor":"green","fontFamily":"Arial","fontSize":"16px","tertiaryColor":"hsl(78.1578947368, 58.4615384615%, 84.5098039216%)","primaryBorderColor":"hsl(78.1578947368, 18.4615384615%, 64.5098039216%)","secondaryBorderColor":"hsl(98.961038961, 60%, 74.9019607843%)","tertiaryBorderColor":"hsl(78.1578947368, 18.4615384615%, 74.5098039216%)","primaryTextColor":"#321b67","secondaryTextColor":"#32004d","tertiaryTextColor":"#321b67","textColor":"#000000","THEME_COLOR_LIMIT":12,"radius":5,"strokeWidth":1,"nodeBkg":"#cde498","nodeBorder":"#225577","clusterBkg":"#cdffb2","clusterBorder":"#6eaa49","defaultLinkColor":"#000000","titleColor":"#333","edgeLabelBackground":"#e8e8e8","actorBorder":"hsl(78.1578947368, 58.4615384615%, 54.5098039216%)","actorBkg":"#cde498","actorTextColor":"black","actorLineColor":"hsl(78.1578947368, 58.4615384615%, 54.5098039216%)","signalColor":"#333","signalTextColor":"#333","labelBoxBkgColor":"#cde498","labelBoxBorderColor":"#326932","labelTextColor":"black","loopTextColor":"black","noteBorderColor":"#6eaa49","noteBkgColor":"#fff5ad","noteTextColor":"black","activationBorderColor":"#666","activationBkgColor":"#f4f4f4","sequenceNumberColor":"white","sectionBkgColor":"#6eaa49","altSectionBkgColor":"white","sectionBkgColor2":"#6eaa49","excludeBkgColor":"#eeeeee","taskBorderColor":"#13540c","taskBkgColor":"#487e3a","taskTextLightColor":"white","taskTextColor":"white","taskTextDarkColor":"black","taskTextOutsideColor":"black","taskTextClickableColor":"#003163","activeTaskBorderColor":"#13540c","activeTaskBkgColor":"#cde498","gridColor":"lightgrey","doneTaskBkgColor":"lightgrey","doneTaskBorderColor":"grey","critBorderColor":"#ff8888","critBkgColor":"red","todayLineColor":"red","vertLineColor":"#00BFFF","personBorder":"hsl(78.1578947368, 18.4615384615%, 64.5098039216%)","personBkg":"#cde498","archEdgeColor":"#000000","archEdgeArrowColor":"#000000","archEdgeWidth":"3","archGroupBorderColor":"hsl(78.1578947368, 18.4615384615%, 64.5098039216%)","archGroupBorderWidth":"2px","noteFontWeight":"normal","fontWeight":"normal","labelColor":"black","errorBkgColor":"#552222","errorTextColor":"#552222","useGradient":true,"gradientStart":"hsl(78.1578947368, 18.4615384615%, 64.5098039216%)","gradientStop":"hsl(98.961038961, 60%, 74.9019607843%)","dropShadow":"drop-shadow( 1px 2px 2px rgba(185,185,185,0.5))","cScale0":"#123456","rectBkgColor":"hsl(78.1578947368, 58.4615384615%, 84.5098039216%)","cScale1":"hsl(98.961038961, 100%, 74.9019607843%)","cScale2":"hsl(78.1578947368, 58.4615384615%, 74.5098039216%)","cScale3":"hsl(108.1578947368, 58.4615384615%, 64.5098039216%)","cScale4":"hsl(138.1578947368, 58.4615384615%, 64.5098039216%)","cScale5":"hsl(168.1578947368, 58.4615384615%, 64.5098039216%)","cScale6":"hsl(198.1578947368, 58.4615384615%, 64.5098039216%)","cScale7":"hsl(228.1578947368, 58.4615384615%, 64.5098039216%)","cScale8":"hsl(288.1578947368, 58.4615384615%, 64.5098039216%)","cScale9":"hsl(348.1578947368, 58.4615384615%, 64.5098039216%)","cScale10":"hsl(18.1578947368, 58.4615384615%, 64.5098039216%)","cScale11":"hsl(48.1578947368, 58.4615384615%, 64.5098039216%)","cScalePeer1":"hsl(98.961038961, 100%, 39.9019607843%)","cScalePeer2":"hsl(78.1578947368, 58.4615384615%, 44.5098039216%)","cScalePeer0":"hsl(210, 65.3846153846%, 0%)","cScalePeer3":"hsl(108.1578947368, 58.4615384615%, 39.5098039216%)","cScalePeer4":"hsl(138.1578947368, 58.4615384615%, 39.5098039216%)","cScalePeer5":"hsl(168.1578947368, 58.4615384615%, 39.5098039216%)","cScalePeer6":"hsl(198.1578947368, 58.4615384615%, 39.5098039216%)","cScalePeer7":"hsl(228.1578947368, 58.4615384615%, 39.5098039216%)","cScalePeer8":"hsl(288.1578947368, 58.4615384615%, 39.5098039216%)","cScalePeer9":"hsl(348.1578947368, 58.4615384615%, 39.5098039216%)","cScalePeer10":"hsl(18.1578947368, 58.4615384615%, 39.5098039216%)","cScalePeer11":"hsl(48.1578947368, 58.4615384615%, 39.5098039216%)","cScaleInv0":"hsl(30, 65.3846153846%, 10.3921568627%)","cScaleInv1":"hsl(278.961038961, 100%, 74.9019607843%)","cScaleInv2":"hsl(258.1578947368, 58.4615384615%, 74.5098039216%)","cScaleInv3":"hsl(288.1578947368, 58.4615384615%, 64.5098039216%)","cScaleInv4":"hsl(318.1578947368, 58.4615384615%, 64.5098039216%)","cScaleInv5":"hsl(348.1578947368, 58.4615384615%, 64.5098039216%)","cScaleInv6":"hsl(18.1578947368, 58.4615384615%, 64.5098039216%)","cScaleInv7":"hsl(48.1578947368, 58.4615384615%, 64.5098039216%)","cScaleInv8":"hsl(108.1578947368, 58.4615384615%, 64.5098039216%)","cScaleInv9":"hsl(168.1578947368, 58.4615384615%, 64.5098039216%)","cScaleInv10":"hsl(198.1578947368, 58.4615384615%, 64.5098039216%)","cScaleInv11":"hsl(228.1578947368, 58.4615384615%, 64.5098039216%)","scaleLabelColor":"black","cScaleLabel0":"black","cScaleLabel1":"black","cScaleLabel2":"black","cScaleLabel3":"black","cScaleLabel4":"black","cScaleLabel5":"black","cScaleLabel6":"black","cScaleLabel7":"black","cScaleLabel8":"black","cScaleLabel9":"black","cScaleLabel10":"black","cScaleLabel11":"black","surface0":"hsl(108.1578947368, 28.4615384615%, 69.5098039216%)","surfacePeer0":"hsl(108.1578947368, 28.4615384615%, 66.5098039216%)","surface1":"hsl(108.1578947368, 28.4615384615%, 64.5098039216%)","surfacePeer1":"hsl(108.1578947368, 28.4615384615%, 61.5098039216%)","surface2":"hsl(108.1578947368, 28.4615384615%, 59.5098039216%)","surfacePeer2":"hsl(108.1578947368, 28.4615384615%, 56.5098039216%)","surface3":"hsl(108.1578947368, 28.4615384615%, 54.5098039216%)","surfacePeer3":"hsl(108.1578947368, 28.4615384615%, 51.5098039216%)","surface4":"hsl(108.1578947368, 28.4615384615%, 49.5098039216%)","surfacePeer4":"hsl(108.1578947368, 28.4615384615%, 46.5098039216%)","flowContainerStroke":"hsl(98.961038961, 60%, 74.9019607843%)","rowOdd":"hsl(78.1578947368, 58.4615384615%, 100%)","rowEven":"hsl(78.1578947368, 58.4615384615%, 94.5098039216%)","transitionColor":"#000000","transitionLabelColor":"#000000","stateLabelColor":"#321b67","stateBkg":"#cde498","labelBackgroundColor":"#cde498","compositeBackground":"white","altBackground":"#f0f0f0","compositeTitleBackground":"#cde498","compositeBorder":"#13540c","innerEndBackground":"hsl(78.1578947368, 18.4615384615%, 64.5098039216%)","specialStateColor":"#000000","classText":"#321b67","fillType0":"#cde498","fillType1":"#cdffb2","fillType2":"hsl(142.1578947368, 58.4615384615%, 74.5098039216%)","fillType3":"hsl(162.961038961, 100%, 84.9019607843%)","fillType4":"hsl(14.1578947368, 58.4615384615%, 74.5098039216%)","fillType5":"hsl(34.961038961, 100%, 84.9019607843%)","fillType6":"hsl(206.1578947368, 58.4615384615%, 74.5098039216%)","fillType7":"hsl(226.961038961, 100%, 84.9019607843%)","pie1":"#cde498","pie2":"#cdffb2","pie3":"hsl(78.1578947368, 58.4615384615%, 84.5098039216%)","pie4":"hsl(78.1578947368, 58.4615384615%, 44.5098039216%)","pie5":"hsl(98.961038961, 100%, 54.9019607843%)","pie6":"hsl(118.1578947368, 58.4615384615%, 44.5098039216%)","pie7":"hsl(138.1578947368, 58.4615384615%, 64.5098039216%)","pie8":"hsl(18.1578947368, 58.4615384615%, 64.5098039216%)","pie9":"hsl(198.1578947368, 58.4615384615%, 74.5098039216%)","pie10":"hsl(138.1578947368, 58.4615384615%, 24.5098039216%)","pie11":"hsl(18.1578947368, 58.4615384615%, 24.5098039216%)","pie12":"hsl(198.1578947368, 58.4615384615%, 24.5098039216%)","pieTitleTextSize":"25px","pieTitleTextColor":"black","pieSectionTextSize":"17px","pieSectionTextColor":"#000000","pieLegendTextSize":"17px","pieLegendTextColor":"black","pieStrokeColor":"black","pieStrokeWidth":"2px","pieOuterStrokeWidth":"2px","pieOuterStrokeColor":"black","pieOpacity":"0.7","venn1":"hsl(78.1578947368, 58.4615384615%, 44.5098039216%)","venn2":"hsl(98.961038961, 100%, 54.9019607843%)","venn3":"hsl(78.1578947368, 58.4615384615%, 54.5098039216%)","venn4":"hsl(138.1578947368, 58.4615384615%, 44.5098039216%)","venn5":"hsl(18.1578947368, 58.4615384615%, 44.5098039216%)","venn6":"hsl(158.961038961, 100%, 54.9019607843%)","venn7":"hsl(198.1578947368, 58.4615384615%, 44.5098039216%)","venn8":"hsl(218.961038961, 100%, 54.9019607843%)","vennTitleTextColor":"#333","vennSetTextColor":"#000000","cynefin":{"domainFontSize":16,"itemFontSize":12,"boundaryColor":"#000000","boundaryWidth":2,"cliffColor":"#8B4513","cliffWidth":4,"arrowColor":"#000000","arrowWidth":2,"complexBg":"#C8E6C9","complicatedBg":"#DCEDC8","chaoticBg":"#FFE0B2","clearBg":"#FFF9C4","confusionBg":"#D7CCC8","textColor":"#000000","labelColor":"#321b67"},"quadrant1Fill":"#cde498","quadrant2Fill":"#d2e99d","quadrant3Fill":"#d7eea2","quadrant4Fill":"#dcf3a7","quadrant1TextFill":"#321b67","quadrant2TextFill":"#2d1662","quadrant3TextFill":"#28115d","quadrant4TextFill":"#230c58","quadrantPointFill":"hsl(78.1578947368, 58.4615384615%, NaN%)","quadrantPointTextFill":"#321b67","quadrantXAxisTextFill":"#321b67","quadrantYAxisTextFill":"#321b67","quadrantInternalBorderStrokeFill":"hsl(78.1578947368, 18.4615384615%, 64.5098039216%)","quadrantExternalBorderStrokeFill":"hsl(78.1578947368, 18.4615384615%, 64.5098039216%)","quadrantTitleFill":"#321b67","packet":{"startByteColor":"#321b67","endByteColor":"#321b67","labelColor":"#321b67","titleColor":"#321b67","blockStrokeColor":"#321b67","blockFillColor":"#cde498"},"radar":{"axisColor":"#000000","axisStrokeWidth":2,"axisLabelFontSize":12,"curveOpacity":0.5,"curveStrokeWidth":2,"graticuleColor":"#DEDEDE","graticuleStrokeWidth":1,"graticuleOpacity":0.3,"legendBoxSize":12,"legendFontSize":12},"wardleyEvolutionColor":"#dc3545","wardley":{"backgroundColor":"white","axisColor":"#000000","axisTextColor":"#321b67","gridColor":"lightgrey","componentFill":"white","componentStroke":"#000000","componentLabelColor":"#321b67","linkStroke":"#000000","evolutionStroke":"#dc3545","annotationStroke":"#000000","annotationTextColor":"#321b67","annotationFill":"white"},"xyChart":{"backgroundColor":"white","titleColor":"#321b67","dataLabelColor":"#321b67","legendTextColor":"#321b67","xAxisTitleColor":"#321b67","xAxisLabelColor":"#321b67","xAxisTickColor":"#321b67","xAxisLineColor":"#321b67","yAxisTitleColor":"#321b67","yAxisLabelColor":"#321b67","yAxisTickColor":"#321b67","yAxisLineColor":"#321b67","plotColorPalette":"#CDE498,#FF6B6B,#A0D2DB,#D7BDE2,#F0F0F0,#FFC3A0,#7FD8BE,#FF9A8B,#FAF3E0,#FFF176"},"requirementBackground":"#cde498","requirementBorderColor":"hsl(78.1578947368, 18.4615384615%, 64.5098039216%)","requirementBorderSize":"1","requirementTextColor":"#321b67","relationColor":"#000000","relationLabelBackground":"#e8e8e8","relationLabelColor":"black","git0":"hsl(78.1578947368, 58.4615384615%, 49.5098039216%)","git1":"hsl(98.961038961, 100%, 59.9019607843%)","git2":"hsl(78.1578947368, 58.4615384615%, 59.5098039216%)","git3":"hsl(48.1578947368, 58.4615384615%, 49.5098039216%)","git4":"hsl(18.1578947368, 58.4615384615%, 49.5098039216%)","git5":"hsl(-11.8421052632, 58.4615384615%, 49.5098039216%)","git6":"hsl(138.1578947368, 58.4615384615%, 49.5098039216%)","git7":"hsl(198.1578947368, 58.4615384615%, 49.5098039216%)","gitInv0":"rgb(99.6153846152, 54.9423076922, 202.5576923076)","gitInv1":"rgb(132.7922077921, 0, 204.5000000001)","gitInv2":"rgb(79.4230769229, 42.8884615385, 163.6115384614)","gitInv3":"rgb(54.9423076922, 84.0769230769, 202.5576923076)","gitInv4":"rgb(54.9423076922, 157.8846153846, 202.5576923076)","gitInv5":"rgb(54.9423076922, 202.5576923076, 173.4230769229)","gitInv6":"rgb(202.5576923076, 54.9423076922, 157.8846153846)","gitInv7":"rgb(202.5576923076, 99.6153846152, 54.9423076922)","gitBranchLabel0":"#ffffff","gitBranchLabel1":"black","gitBranchLabel2":"black","gitBranchLabel3":"#ffffff","gitBranchLabel4":"black","gitBranchLabel5":"black","gitBranchLabel6":"black","gitBranchLabel7":"black","tagLabelColor":"#321b67","tagLabelBackground":"#cde498","tagLabelBorder":"hsl(78.1578947368, 18.4615384615%, 64.5098039216%)","tagLabelFontSize":"10px","commitLabelColor":"#32004d","commitLabelBackground":"#cdffb2","commitLabelFontSize":"10px","emUiFill":"white","emUiStroke":"#dbdada","emProcessorFill":"#edb3f6","emProcessorStroke":"#b88cbf","emReadModelFill":"#d3f1a2","emReadModelStroke":"#a3b732","emCommandFill":"#bcd6fe","emCommandStroke":"#679ac3","emEventFill":"#ffb778","emEventStroke":"#c19a0f","emSwimlaneBackgroundOdd":"rgb(250,250,250)","emSwimlaneBackgroundStroke":"rgb(240,240,240)","emArrowhead":"#000000","emRelationStroke":"#000000","attributeBackgroundColorOdd":"#ffffff","attributeBackgroundColorEven":"#f2f2f2"}"##;
    const NEUTRAL_NONE: &str = r##"{"primaryColor":"#eee","contrast":"#707070","secondaryColor":"hsl(0, 0%, 98.9215686275%)","background":"#ffffff","tertiaryColor":"hsl(-160, 0%, 93.3333333333%)","primaryBorderColor":"hsl(0, 0%, 83.3333333333%)","secondaryBorderColor":"hsl(0, 0%, 88.9215686275%)","tertiaryBorderColor":"hsl(-160, 0%, 83.3333333333%)","primaryTextColor":"#111111","secondaryTextColor":"rgb(2.7499999999, 2.7499999999, 2.7499999999)","tertiaryTextColor":"rgb(17.0000000001, 17.0000000001, 17.0000000001)","lineColor":"#666","textColor":"#000000","mainBkg":"#eee","secondBkg":"hsl(0, 0%, 98.9215686275%)","border1":"#999","border2":"#707070","note":"#ffa","text":"#333","critical":"#d42","done":"#bbb","arrowheadColor":"#333333","fontFamily":"\"trebuchet ms\", verdana, arial, sans-serif","fontSize":"16px","THEME_COLOR_LIMIT":12,"radius":5,"strokeWidth":1,"nodeBkg":"#eee","nodeBorder":"#999","clusterBkg":"hsl(0, 0%, 98.9215686275%)","clusterBorder":"#707070","defaultLinkColor":"#666","titleColor":"#333","edgeLabelBackground":"white","actorBorder":"hsl(0, 0%, 83%)","actorBkg":"#eee","actorTextColor":"#333","actorLineColor":"hsl(0, 0%, 83%)","signalColor":"#333","signalTextColor":"#333","labelBoxBkgColor":"#eee","labelBoxBorderColor":"hsl(0, 0%, 83%)","labelTextColor":"#333","loopTextColor":"#333","noteBorderColor":"#999","noteBkgColor":"#666","noteTextColor":"#fff","activationBorderColor":"#666","activationBkgColor":"#f4f4f4","sequenceNumberColor":"white","sectionBkgColor":"hsl(0, 0%, 73.9215686275%)","altSectionBkgColor":"white","sectionBkgColor2":"hsl(0, 0%, 73.9215686275%)","excludeBkgColor":"#eeeeee","taskBorderColor":"hsl(0, 0%, 33.9215686275%)","taskBkgColor":"#707070","taskTextLightColor":"white","taskTextColor":"white","taskTextDarkColor":"#333","taskTextOutsideColor":"#333","taskTextClickableColor":"#003163","activeTaskBorderColor":"hsl(0, 0%, 33.9215686275%)","activeTaskBkgColor":"#eee","gridColor":"hsl(0, 0%, 90%)","doneTaskBkgColor":"#bbb","doneTaskBorderColor":"#666","critBkgColor":"#d42","critBorderColor":"hsl(10.9090909091, 73.3333333333%, 40%)","todayLineColor":"#d42","vertLineColor":"#d42","personBorder":"hsl(0, 0%, 83.3333333333%)","personBkg":"#eee","archEdgeColor":"#666","archEdgeArrowColor":"#666","archEdgeWidth":"3","archGroupBorderColor":"hsl(0, 0%, 83.3333333333%)","archGroupBorderWidth":"2px","noteFontWeight":"normal","fontWeight":"normal","rowOdd":"hsl(0, 0%, 100%)","rowEven":"#f4f4f4","labelColor":"black","errorBkgColor":"#552222","errorTextColor":"#552222","useGradient":true,"gradientStart":"hsl(0, 0%, 83.3333333333%)","gradientStop":"hsl(0, 0%, 88.9215686275%)","dropShadow":"drop-shadow( 1px 2px 2px rgba(185,185,185,1))","rectBkgColor":"hsl(-160, 0%, 93.3333333333%)","cScale0":"#555","cScale1":"#F4F4F4","cScale2":"#555","cScale3":"#BBB","cScale4":"#777","cScale5":"#999","cScale6":"#DDD","cScale7":"#FFF","cScale8":"#DDD","cScale9":"#BBB","cScale10":"#999","cScale11":"#777","cScaleInv0":"#aaaaaa","cScaleInv1":"#0b0b0b","cScaleInv2":"#aaaaaa","cScaleInv3":"#444444","cScaleInv4":"#888888","cScaleInv5":"#666666","cScaleInv6":"#222222","cScaleInv7":"#000000","cScaleInv8":"#222222","cScaleInv9":"#444444","cScaleInv10":"#666666","cScaleInv11":"#888888","cScalePeer0":"hsl(0, 0%, 23.3333333333%)","cScalePeer1":"hsl(0, 0%, 85.6862745098%)","cScalePeer2":"hsl(0, 0%, 23.3333333333%)","cScalePeer3":"hsl(0, 0%, 63.3333333333%)","cScalePeer4":"hsl(0, 0%, 36.6666666667%)","cScalePeer5":"hsl(0, 0%, 50%)","cScalePeer6":"hsl(0, 0%, 76.6666666667%)","cScalePeer7":"hsl(0, 0%, 90%)","cScalePeer8":"hsl(0, 0%, 76.6666666667%)","cScalePeer9":"hsl(0, 0%, 63.3333333333%)","cScalePeer10":"hsl(0, 0%, 50%)","cScalePeer11":"hsl(0, 0%, 36.6666666667%)","scaleLabelColor":"#333","cScaleLabel0":"#F4F4F4","cScaleLabel2":"#F4F4F4","cScaleLabel1":"#333","cScaleLabel3":"#333","cScaleLabel4":"#333","cScaleLabel5":"#333","cScaleLabel6":"#333","cScaleLabel7":"#333","cScaleLabel8":"#333","cScaleLabel9":"#333","cScaleLabel10":"#333","cScaleLabel11":"#333","surface0":"hsl(0, 0%, 88.3333333333%)","surfacePeer0":"hsl(0, 0%, 85.3333333333%)","surface1":"hsl(0, 0%, 83.3333333333%)","surfacePeer1":"hsl(0, 0%, 80.3333333333%)","surface2":"hsl(0, 0%, 78.3333333333%)","surfacePeer2":"hsl(0, 0%, 75.3333333333%)","surface3":"hsl(0, 0%, 73.3333333333%)","surfacePeer3":"hsl(0, 0%, 70.3333333333%)","surface4":"hsl(0, 0%, 68.3333333333%)","surfacePeer4":"hsl(0, 0%, 65.3333333333%)","flowContainerStroke":"hsl(0, 0%, 88.9215686275%)","transitionColor":"#000","transitionLabelColor":"#000000","stateLabelColor":"#111111","stateBkg":"#eee","labelBackgroundColor":"#eee","compositeBackground":"#ffffff","altBackground":"#f4f4f4","compositeTitleBackground":"#eee","stateBorder":"#000","innerEndBackground":"hsl(0, 0%, 83.3333333333%)","specialStateColor":"#222","classText":"#111111","fillType0":"#eee","fillType1":"hsl(0, 0%, 98.9215686275%)","fillType2":"hsl(64, 0%, 93.3333333333%)","fillType3":"hsl(64, 0%, 98.9215686275%)","fillType4":"hsl(-64, 0%, 93.3333333333%)","fillType5":"hsl(-64, 0%, 98.9215686275%)","fillType6":"hsl(128, 0%, 93.3333333333%)","fillType7":"hsl(128, 0%, 98.9215686275%)","pie0":"#555","pie1":"#F4F4F4","pie2":"#555","pie3":"#BBB","pie4":"#777","pie5":"#999","pie6":"#DDD","pie7":"#FFF","pie8":"#DDD","pie9":"#BBB","pie10":"#999","pie11":"#777","pie12":"#555","pieTitleTextSize":"25px","pieTitleTextColor":"#333","pieSectionTextSize":"17px","pieSectionTextColor":"#000000","pieLegendTextSize":"17px","pieLegendTextColor":"#333","pieStrokeColor":"black","pieStrokeWidth":"2px","pieOuterStrokeWidth":"2px","pieOuterStrokeColor":"black","pieOpacity":"0.7","venn1":"#555","venn2":"#F4F4F4","venn3":"#555","venn4":"#BBB","venn5":"#777","venn6":"#999","venn7":"#DDD","venn8":"#FFF","vennTitleTextColor":"#333","vennSetTextColor":"#000000","cynefin":{"domainFontSize":16,"itemFontSize":12,"boundaryColor":"#666","boundaryWidth":2,"cliffColor":"#8B0000","cliffWidth":4,"arrowColor":"#666","arrowWidth":2,"complexBg":"#E8F5E9","complicatedBg":"#E3F2FD","chaoticBg":"#FBE9E7","clearBg":"#FFF8E1","confusionBg":"#F3E5F5","textColor":"#000000","labelColor":"#111111"},"quadrant1Fill":"#eee","quadrant2Fill":"#f3f3f3","quadrant3Fill":"#f8f8f8","quadrant4Fill":"#fdfdfd","quadrant1TextFill":"#111111","quadrant2TextFill":"#0c0c0c","quadrant3TextFill":"#070707","quadrant4TextFill":"#020202","quadrantPointFill":"hsl(0, 0%, NaN%)","quadrantPointTextFill":"#111111","quadrantXAxisTextFill":"#111111","quadrantYAxisTextFill":"#111111","quadrantInternalBorderStrokeFill":"hsl(0, 0%, 83.3333333333%)","quadrantExternalBorderStrokeFill":"hsl(0, 0%, 83.3333333333%)","quadrantTitleFill":"#111111","xyChart":{"backgroundColor":"#ffffff","titleColor":"#111111","dataLabelColor":"#111111","legendTextColor":"#111111","xAxisTitleColor":"#111111","xAxisLabelColor":"#111111","xAxisTickColor":"#111111","xAxisLineColor":"#111111","yAxisTitleColor":"#111111","yAxisLabelColor":"#111111","yAxisTickColor":"#111111","yAxisLineColor":"#111111","plotColorPalette":"#EEE,#6BB8E4,#8ACB88,#C7ACD6,#E8DCC2,#FFB2A8,#FFF380,#7E8D91,#FFD8B1,#FAF3E0"},"radar":{"axisColor":"#666","axisStrokeWidth":2,"axisLabelFontSize":12,"curveOpacity":0.5,"curveStrokeWidth":2,"graticuleColor":"#DEDEDE","graticuleStrokeWidth":1,"graticuleOpacity":0.3,"legendBoxSize":12,"legendFontSize":12},"wardleyEvolutionColor":"#dc3545","wardley":{"backgroundColor":"#ffffff","axisColor":"#666","axisTextColor":"#111111","gridColor":"hsl(0, 0%, 90%)","componentFill":"#ffffff","componentStroke":"#666","componentLabelColor":"#111111","linkStroke":"#666","evolutionStroke":"#dc3545","annotationStroke":"#666","annotationTextColor":"#111111","annotationFill":"#ffffff"},"requirementBackground":"#eee","requirementBorderColor":"hsl(0, 0%, 83.3333333333%)","requirementBorderSize":"1","requirementTextColor":"#111111","relationColor":"#666","relationLabelBackground":"white","relationLabelColor":"#333","git0":"hsl(0, 0%, 70.6862745098%)","git1":"#555","git2":"#BBB","git3":"#777","git4":"#999","git5":"#DDD","git6":"#FFF","git7":"#DDD","gitInv0":"rgb(74.75, 74.75, 74.75)","gitInv1":"#aaaaaa","gitInv2":"#444444","gitInv3":"#888888","gitInv4":"#666666","gitInv5":"#222222","gitInv6":"#000000","gitInv7":"#222222","branchLabelColor":"#333","gitBranchLabel0":"#333","gitBranchLabel1":"white","gitBranchLabel2":"#333","gitBranchLabel3":"white","gitBranchLabel4":"#333","gitBranchLabel5":"#333","gitBranchLabel6":"#333","gitBranchLabel7":"#333","tagLabelColor":"#111111","tagLabelBackground":"#eee","tagLabelBorder":"hsl(0, 0%, 83.3333333333%)","tagLabelFontSize":"10px","commitLabelColor":"rgb(2.7499999999, 2.7499999999, 2.7499999999)","commitLabelBackground":"hsl(0, 0%, 98.9215686275%)","commitLabelFontSize":"10px","emUiFill":"white","emUiStroke":"#dbdada","emProcessorFill":"#edb3f6","emProcessorStroke":"#b88cbf","emReadModelFill":"#d3f1a2","emReadModelStroke":"#a3b732","emCommandFill":"#bcd6fe","emCommandStroke":"#679ac3","emEventFill":"#ffb778","emEventStroke":"#c19a0f","emSwimlaneBackgroundOdd":"rgb(250,250,250)","emSwimlaneBackgroundStroke":"rgb(240,240,240)","emArrowhead":"#666","emRelationStroke":"#666","attributeBackgroundColorOdd":"#ffffff","attributeBackgroundColorEven":"#f2f2f2"}"##;
    const NEUTRAL_COLORS: &str = r##"{"primaryColor":"#abcdef","contrast":"#707070","secondaryColor":"#123456","background":"#ffffff","tertiaryColor":"hsl(-160, 0%, 93.3333333333%)","primaryBorderColor":"hsl(0, 0%, 83.3333333333%)","secondaryBorderColor":"hsl(0, 0%, 88.9215686275%)","tertiaryBorderColor":"hsl(-160, 0%, 83.3333333333%)","primaryTextColor":"#111111","secondaryTextColor":"rgb(2.7499999999, 2.7499999999, 2.7499999999)","tertiaryTextColor":"rgb(17.0000000001, 17.0000000001, 17.0000000001)","lineColor":"#666","textColor":"#000000","mainBkg":"#eee","secondBkg":"hsl(0, 0%, 98.9215686275%)","border1":"#999","border2":"#707070","note":"#ffa","text":"#333","critical":"#d42","done":"#bbb","arrowheadColor":"#333333","fontFamily":"\"trebuchet ms\", verdana, arial, sans-serif","fontSize":"16px","THEME_COLOR_LIMIT":12,"radius":5,"strokeWidth":1,"nodeBkg":"#eee","nodeBorder":"#999","clusterBkg":"hsl(0, 0%, 98.9215686275%)","clusterBorder":"#707070","defaultLinkColor":"#666","titleColor":"#333","edgeLabelBackground":"white","actorBorder":"hsl(0, 0%, 83%)","actorBkg":"#eee","actorTextColor":"#333","actorLineColor":"hsl(0, 0%, 83%)","signalColor":"#333","signalTextColor":"#333","labelBoxBkgColor":"#eee","labelBoxBorderColor":"hsl(0, 0%, 83%)","labelTextColor":"#333","loopTextColor":"#333","noteBorderColor":"#999","noteBkgColor":"#666","noteTextColor":"#fff","activationBorderColor":"#666","activationBkgColor":"#f4f4f4","sequenceNumberColor":"white","sectionBkgColor":"hsl(0, 0%, 73.9215686275%)","altSectionBkgColor":"white","sectionBkgColor2":"hsl(0, 0%, 73.9215686275%)","excludeBkgColor":"#eeeeee","taskBorderColor":"hsl(0, 0%, 33.9215686275%)","taskBkgColor":"#707070","taskTextLightColor":"white","taskTextColor":"white","taskTextDarkColor":"#333","taskTextOutsideColor":"#333","taskTextClickableColor":"#003163","activeTaskBorderColor":"hsl(0, 0%, 33.9215686275%)","activeTaskBkgColor":"#eee","gridColor":"hsl(0, 0%, 90%)","doneTaskBkgColor":"#bbb","doneTaskBorderColor":"#666","critBkgColor":"#d42","critBorderColor":"hsl(10.9090909091, 73.3333333333%, 40%)","todayLineColor":"#d42","vertLineColor":"#d42","personBorder":"hsl(0, 0%, 83.3333333333%)","personBkg":"#eee","archEdgeColor":"#666","archEdgeArrowColor":"#666","archEdgeWidth":"3","archGroupBorderColor":"hsl(0, 0%, 83.3333333333%)","archGroupBorderWidth":"2px","noteFontWeight":"normal","fontWeight":"normal","rowOdd":"hsl(0, 0%, 100%)","rowEven":"#f4f4f4","labelColor":"black","errorBkgColor":"#552222","errorTextColor":"#552222","useGradient":true,"gradientStart":"hsl(0, 0%, 83.3333333333%)","gradientStop":"hsl(0, 0%, 88.9215686275%)","dropShadow":"drop-shadow( 1px 2px 2px rgba(185,185,185,1))","darkMode":true,"rectBkgColor":"hsl(-160, 0%, 93.3333333333%)","cScale0":"#555","cScale1":"#F4F4F4","cScale2":"#555","cScale3":"#BBB","cScale4":"#777","cScale5":"#999","cScale6":"#DDD","cScale7":"#FFF","cScale8":"#DDD","cScale9":"#BBB","cScale10":"#999","cScale11":"#777","cScaleInv0":"#aaaaaa","cScaleInv1":"#0b0b0b","cScaleInv2":"#aaaaaa","cScaleInv3":"#444444","cScaleInv4":"#888888","cScaleInv5":"#666666","cScaleInv6":"#222222","cScaleInv7":"#000000","cScaleInv8":"#222222","cScaleInv9":"#444444","cScaleInv10":"#666666","cScaleInv11":"#888888","cScalePeer0":"hsl(0, 0%, 43.3333333333%)","cScalePeer1":"hsl(0, 0%, 100%)","cScalePeer2":"hsl(0, 0%, 43.3333333333%)","cScalePeer3":"hsl(0, 0%, 83.3333333333%)","cScalePeer4":"hsl(0, 0%, 56.6666666667%)","cScalePeer5":"hsl(0, 0%, 70%)","cScalePeer6":"hsl(0, 0%, 96.6666666667%)","cScalePeer7":"#FFF","cScalePeer8":"hsl(0, 0%, 96.6666666667%)","cScalePeer9":"hsl(0, 0%, 83.3333333333%)","cScalePeer10":"hsl(0, 0%, 70%)","cScalePeer11":"hsl(0, 0%, 56.6666666667%)","scaleLabelColor":"black","cScaleLabel0":"#F4F4F4","cScaleLabel2":"#F4F4F4","cScaleLabel1":"black","cScaleLabel3":"black","cScaleLabel4":"black","cScaleLabel5":"black","cScaleLabel6":"black","cScaleLabel7":"black","cScaleLabel8":"black","cScaleLabel9":"black","cScaleLabel10":"black","cScaleLabel11":"black","surface0":"hsl(0, 0%, 88.3333333333%)","surfacePeer0":"hsl(0, 0%, 85.3333333333%)","surface1":"hsl(0, 0%, 83.3333333333%)","surfacePeer1":"hsl(0, 0%, 80.3333333333%)","surface2":"hsl(0, 0%, 78.3333333333%)","surfacePeer2":"hsl(0, 0%, 75.3333333333%)","surface3":"hsl(0, 0%, 73.3333333333%)","surfacePeer3":"hsl(0, 0%, 70.3333333333%)","surface4":"hsl(0, 0%, 68.3333333333%)","surfacePeer4":"hsl(0, 0%, 65.3333333333%)","flowContainerStroke":"hsl(0, 0%, 88.9215686275%)","transitionColor":"#000","transitionLabelColor":"#000000","stateLabelColor":"#111111","stateBkg":"#eee","labelBackgroundColor":"#eee","compositeBackground":"#ffffff","altBackground":"#f4f4f4","compositeTitleBackground":"#eee","stateBorder":"#000","innerEndBackground":"hsl(0, 0%, 83.3333333333%)","specialStateColor":"#222","classText":"#111111","fillType0":"#abcdef","fillType1":"#123456","fillType2":"hsl(274, 68%, 80.3921568627%)","fillType3":"hsl(274, 65.3846153846%, 20.3921568627%)","fillType4":"hsl(146, 68%, 80.3921568627%)","fillType5":"hsl(146, 65.3846153846%, 20.3921568627%)","fillType6":"hsl(338, 68%, 80.3921568627%)","fillType7":"hsl(338, 65.3846153846%, 20.3921568627%)","pie0":"#555","pie1":"#F4F4F4","pie2":"#555","pie3":"#BBB","pie4":"#777","pie5":"#999","pie6":"#DDD","pie7":"#FFF","pie8":"#DDD","pie9":"#BBB","pie10":"#999","pie11":"#777","pie12":"#555","pieTitleTextSize":"25px","pieTitleTextColor":"#333","pieSectionTextSize":"17px","pieSectionTextColor":"#000000","pieLegendTextSize":"17px","pieLegendTextColor":"#333","pieStrokeColor":"black","pieStrokeWidth":"2px","pieOuterStrokeWidth":"2px","pieOuterStrokeColor":"black","pieOpacity":"0.7","venn1":"#555","venn2":"#F4F4F4","venn3":"#555","venn4":"#BBB","venn5":"#777","venn6":"#999","venn7":"#DDD","venn8":"#FFF","vennTitleTextColor":"#333","vennSetTextColor":"#000000","cynefin":{"domainFontSize":16,"itemFontSize":12,"boundaryColor":"#666","boundaryWidth":2,"cliffColor":"#8B0000","cliffWidth":4,"arrowColor":"#666","arrowWidth":2,"complexBg":"#E8F5E9","complicatedBg":"#E3F2FD","chaoticBg":"#FBE9E7","clearBg":"#FFF8E1","confusionBg":"#F3E5F5","textColor":"#000000","labelColor":"#111111"},"quadrant1Fill":"#abcdef","quadrant2Fill":"#b0d2f4","quadrant3Fill":"#b5d7f9","quadrant4Fill":"#badcfe","quadrant1TextFill":"#111111","quadrant2TextFill":"#0c0c0c","quadrant3TextFill":"#070707","quadrant4TextFill":"#020202","quadrantPointFill":"hsl(210, 68%, NaN%)","quadrantPointTextFill":"#111111","quadrantXAxisTextFill":"#111111","quadrantYAxisTextFill":"#111111","quadrantInternalBorderStrokeFill":"hsl(0, 0%, 83.3333333333%)","quadrantExternalBorderStrokeFill":"hsl(0, 0%, 83.3333333333%)","quadrantTitleFill":"#111111","xyChart":{"backgroundColor":"#ffffff","titleColor":"#111111","dataLabelColor":"#111111","legendTextColor":"#111111","xAxisTitleColor":"#111111","xAxisLabelColor":"#111111","xAxisTickColor":"#111111","xAxisLineColor":"#111111","yAxisTitleColor":"#111111","yAxisLabelColor":"#111111","yAxisTickColor":"#111111","yAxisLineColor":"#111111","plotColorPalette":"#EEE,#6BB8E4,#8ACB88,#C7ACD6,#E8DCC2,#FFB2A8,#FFF380,#7E8D91,#FFD8B1,#FAF3E0"},"radar":{"axisColor":"#666","axisStrokeWidth":2,"axisLabelFontSize":12,"curveOpacity":0.5,"curveStrokeWidth":2,"graticuleColor":"#DEDEDE","graticuleStrokeWidth":1,"graticuleOpacity":0.3,"legendBoxSize":12,"legendFontSize":12},"wardleyEvolutionColor":"#dc3545","wardley":{"backgroundColor":"#ffffff","axisColor":"#666","axisTextColor":"#111111","gridColor":"hsl(0, 0%, 90%)","componentFill":"#ffffff","componentStroke":"#666","componentLabelColor":"#111111","linkStroke":"#666","evolutionStroke":"#dc3545","annotationStroke":"#666","annotationTextColor":"#111111","annotationFill":"#ffffff"},"requirementBackground":"#abcdef","requirementBorderColor":"hsl(0, 0%, 83.3333333333%)","requirementBorderSize":"1","requirementTextColor":"#111111","relationColor":"#666","relationLabelBackground":"white","relationLabelColor":"#333","git0":"hsl(0, 0%, 70.6862745098%)","git1":"#555","git2":"#BBB","git3":"#777","git4":"#999","git5":"#DDD","git6":"#FFF","git7":"#DDD","gitInv0":"rgb(74.75, 74.75, 74.75)","gitInv1":"#aaaaaa","gitInv2":"#444444","gitInv3":"#888888","gitInv4":"#666666","gitInv5":"#222222","gitInv6":"#000000","gitInv7":"#222222","branchLabelColor":"#333","gitBranchLabel0":"#333","gitBranchLabel1":"white","gitBranchLabel2":"#333","gitBranchLabel3":"white","gitBranchLabel4":"#333","gitBranchLabel5":"#333","gitBranchLabel6":"#333","gitBranchLabel7":"#333","tagLabelColor":"#111111","tagLabelBackground":"#abcdef","tagLabelBorder":"hsl(0, 0%, 83.3333333333%)","tagLabelFontSize":"10px","commitLabelColor":"rgb(2.7499999999, 2.7499999999, 2.7499999999)","commitLabelBackground":"#123456","commitLabelFontSize":"10px","emUiFill":"white","emUiStroke":"#dbdada","emProcessorFill":"#edb3f6","emProcessorStroke":"#b88cbf","emReadModelFill":"#d3f1a2","emReadModelStroke":"#a3b732","emCommandFill":"#bcd6fe","emCommandStroke":"#679ac3","emEventFill":"#ffb778","emEventStroke":"#c19a0f","emSwimlaneBackgroundOdd":"rgb(250,250,250)","emSwimlaneBackgroundStroke":"rgb(240,240,240)","emArrowhead":"#666","emRelationStroke":"#666","attributeBackgroundColorOdd":"#ffffff","attributeBackgroundColorEven":"#f2f2f2"}"##;
    const NEUTRAL_NESTED: &str = r##"{"primaryColor":"#eee","contrast":"#707070","secondaryColor":"hsl(0, 0%, 98.9215686275%)","background":"#ffffff","tertiaryColor":"hsl(-160, 0%, 93.3333333333%)","primaryBorderColor":"hsl(0, 0%, 83.3333333333%)","secondaryBorderColor":"hsl(0, 0%, 88.9215686275%)","tertiaryBorderColor":"hsl(-160, 0%, 83.3333333333%)","primaryTextColor":"#111111","secondaryTextColor":"rgb(2.7499999999, 2.7499999999, 2.7499999999)","tertiaryTextColor":"rgb(17.0000000001, 17.0000000001, 17.0000000001)","lineColor":"#666","textColor":"#000000","mainBkg":"#eee","secondBkg":"hsl(0, 0%, 98.9215686275%)","border1":"#999","border2":"#707070","note":"#ffa","text":"#333","critical":"#d42","done":"#bbb","arrowheadColor":"#333333","fontFamily":"\"trebuchet ms\", verdana, arial, sans-serif","fontSize":"16px","THEME_COLOR_LIMIT":12,"radius":5,"strokeWidth":1,"nodeBkg":"#eee","nodeBorder":"#999","clusterBkg":"hsl(0, 0%, 98.9215686275%)","clusterBorder":"#707070","defaultLinkColor":"#666","titleColor":"#333","edgeLabelBackground":"white","actorBorder":"hsl(0, 0%, 83%)","actorBkg":"#eee","actorTextColor":"#333","actorLineColor":"hsl(0, 0%, 83%)","signalColor":"#333","signalTextColor":"#333","labelBoxBkgColor":"#eee","labelBoxBorderColor":"hsl(0, 0%, 83%)","labelTextColor":"#333","loopTextColor":"#333","noteBorderColor":"#999","noteBkgColor":"#666","noteTextColor":"#fff","activationBorderColor":"#666","activationBkgColor":"#f4f4f4","sequenceNumberColor":"white","sectionBkgColor":"hsl(0, 0%, 73.9215686275%)","altSectionBkgColor":"white","sectionBkgColor2":"hsl(0, 0%, 73.9215686275%)","excludeBkgColor":"#eeeeee","taskBorderColor":"hsl(0, 0%, 33.9215686275%)","taskBkgColor":"#707070","taskTextLightColor":"white","taskTextColor":"white","taskTextDarkColor":"#333","taskTextOutsideColor":"#333","taskTextClickableColor":"#003163","activeTaskBorderColor":"hsl(0, 0%, 33.9215686275%)","activeTaskBkgColor":"#eee","gridColor":"hsl(0, 0%, 90%)","doneTaskBkgColor":"#bbb","doneTaskBorderColor":"#666","critBkgColor":"#d42","critBorderColor":"hsl(10.9090909091, 73.3333333333%, 40%)","todayLineColor":"#d42","vertLineColor":"#d42","personBorder":"hsl(0, 0%, 83.3333333333%)","personBkg":"#eee","archEdgeColor":"#666","archEdgeArrowColor":"#666","archEdgeWidth":"3","archGroupBorderColor":"hsl(0, 0%, 83.3333333333%)","archGroupBorderWidth":"2px","noteFontWeight":"normal","fontWeight":"normal","rowOdd":"hsl(0, 0%, 100%)","rowEven":"#f4f4f4","labelColor":"black","errorBkgColor":"#552222","errorTextColor":"#552222","useGradient":true,"gradientStart":"hsl(0, 0%, 83.3333333333%)","gradientStop":"hsl(0, 0%, 88.9215686275%)","dropShadow":"drop-shadow( 1px 2px 2px rgba(185,185,185,1))","xyChart":{"backgroundColor":"#ffffff","titleColor":"#ff0000","dataLabelColor":"#111111","legendTextColor":"#111111","xAxisTitleColor":"#111111","xAxisLabelColor":"#111111","xAxisTickColor":"#111111","xAxisLineColor":"#111111","yAxisTitleColor":"#111111","yAxisLabelColor":"#111111","yAxisTickColor":"#111111","yAxisLineColor":"#111111","plotColorPalette":"#EEE,#6BB8E4,#8ACB88,#C7ACD6,#E8DCC2,#FFB2A8,#FFF380,#7E8D91,#FFD8B1,#FAF3E0"},"rectBkgColor":"hsl(-160, 0%, 93.3333333333%)","cScale0":"#555","cScale1":"#F4F4F4","cScale2":"#555","cScale3":"#BBB","cScale4":"#777","cScale5":"#999","cScale6":"#DDD","cScale7":"#FFF","cScale8":"#DDD","cScale9":"#BBB","cScale10":"#999","cScale11":"#777","cScaleInv0":"#aaaaaa","cScaleInv1":"#0b0b0b","cScaleInv2":"#aaaaaa","cScaleInv3":"#444444","cScaleInv4":"#888888","cScaleInv5":"#666666","cScaleInv6":"#222222","cScaleInv7":"#000000","cScaleInv8":"#222222","cScaleInv9":"#444444","cScaleInv10":"#666666","cScaleInv11":"#888888","cScalePeer0":"hsl(0, 0%, 23.3333333333%)","cScalePeer1":"hsl(0, 0%, 85.6862745098%)","cScalePeer2":"hsl(0, 0%, 23.3333333333%)","cScalePeer3":"hsl(0, 0%, 63.3333333333%)","cScalePeer4":"hsl(0, 0%, 36.6666666667%)","cScalePeer5":"hsl(0, 0%, 50%)","cScalePeer6":"hsl(0, 0%, 76.6666666667%)","cScalePeer7":"hsl(0, 0%, 90%)","cScalePeer8":"hsl(0, 0%, 76.6666666667%)","cScalePeer9":"hsl(0, 0%, 63.3333333333%)","cScalePeer10":"hsl(0, 0%, 50%)","cScalePeer11":"hsl(0, 0%, 36.6666666667%)","scaleLabelColor":"#333","cScaleLabel0":"#F4F4F4","cScaleLabel2":"#F4F4F4","cScaleLabel1":"#333","cScaleLabel3":"#333","cScaleLabel4":"#333","cScaleLabel5":"#333","cScaleLabel6":"#333","cScaleLabel7":"#333","cScaleLabel8":"#333","cScaleLabel9":"#333","cScaleLabel10":"#333","cScaleLabel11":"#333","surface0":"hsl(0, 0%, 88.3333333333%)","surfacePeer0":"hsl(0, 0%, 85.3333333333%)","surface1":"hsl(0, 0%, 83.3333333333%)","surfacePeer1":"hsl(0, 0%, 80.3333333333%)","surface2":"hsl(0, 0%, 78.3333333333%)","surfacePeer2":"hsl(0, 0%, 75.3333333333%)","surface3":"hsl(0, 0%, 73.3333333333%)","surfacePeer3":"hsl(0, 0%, 70.3333333333%)","surface4":"hsl(0, 0%, 68.3333333333%)","surfacePeer4":"hsl(0, 0%, 65.3333333333%)","flowContainerStroke":"hsl(0, 0%, 88.9215686275%)","transitionColor":"#000","transitionLabelColor":"#000000","stateLabelColor":"#111111","stateBkg":"#eee","labelBackgroundColor":"#eee","compositeBackground":"#ffffff","altBackground":"#f4f4f4","compositeTitleBackground":"#eee","stateBorder":"#000","innerEndBackground":"hsl(0, 0%, 83.3333333333%)","specialStateColor":"#222","classText":"#111111","fillType0":"#eee","fillType1":"hsl(0, 0%, 98.9215686275%)","fillType2":"hsl(64, 0%, 93.3333333333%)","fillType3":"hsl(64, 0%, 98.9215686275%)","fillType4":"hsl(-64, 0%, 93.3333333333%)","fillType5":"hsl(-64, 0%, 98.9215686275%)","fillType6":"hsl(128, 0%, 93.3333333333%)","fillType7":"hsl(128, 0%, 98.9215686275%)","pie0":"#555","pie1":"#F4F4F4","pie2":"#555","pie3":"#BBB","pie4":"#777","pie5":"#999","pie6":"#DDD","pie7":"#FFF","pie8":"#DDD","pie9":"#BBB","pie10":"#999","pie11":"#777","pie12":"#555","pieTitleTextSize":"25px","pieTitleTextColor":"#333","pieSectionTextSize":"17px","pieSectionTextColor":"#000000","pieLegendTextSize":"17px","pieLegendTextColor":"#333","pieStrokeColor":"black","pieStrokeWidth":"2px","pieOuterStrokeWidth":"2px","pieOuterStrokeColor":"black","pieOpacity":"0.7","venn1":"#555","venn2":"#F4F4F4","venn3":"#555","venn4":"#BBB","venn5":"#777","venn6":"#999","venn7":"#DDD","venn8":"#FFF","vennTitleTextColor":"#333","vennSetTextColor":"#000000","cynefin":{"domainFontSize":16,"itemFontSize":12,"boundaryColor":"#666","boundaryWidth":2,"cliffColor":"#8B0000","cliffWidth":4,"arrowColor":"#666","arrowWidth":2,"complexBg":"#E8F5E9","complicatedBg":"#E3F2FD","chaoticBg":"#FBE9E7","clearBg":"#FFF8E1","confusionBg":"#F3E5F5","textColor":"#000000","labelColor":"#111111"},"quadrant1Fill":"#eee","quadrant2Fill":"#f3f3f3","quadrant3Fill":"#f8f8f8","quadrant4Fill":"#fdfdfd","quadrant1TextFill":"#111111","quadrant2TextFill":"#0c0c0c","quadrant3TextFill":"#070707","quadrant4TextFill":"#020202","quadrantPointFill":"hsl(0, 0%, NaN%)","quadrantPointTextFill":"#111111","quadrantXAxisTextFill":"#111111","quadrantYAxisTextFill":"#111111","quadrantInternalBorderStrokeFill":"hsl(0, 0%, 83.3333333333%)","quadrantExternalBorderStrokeFill":"hsl(0, 0%, 83.3333333333%)","quadrantTitleFill":"#111111","radar":{"axisColor":"#666","axisStrokeWidth":2,"axisLabelFontSize":12,"curveOpacity":0.5,"curveStrokeWidth":2,"graticuleColor":"#DEDEDE","graticuleStrokeWidth":1,"graticuleOpacity":0.3,"legendBoxSize":12,"legendFontSize":12},"wardleyEvolutionColor":"#dc3545","wardley":{"backgroundColor":"#ffffff","axisColor":"#666","axisTextColor":"#111111","gridColor":"hsl(0, 0%, 90%)","componentFill":"#ffffff","componentStroke":"#666","componentLabelColor":"#111111","linkStroke":"#666","evolutionStroke":"#dc3545","annotationStroke":"#666","annotationTextColor":"#111111","annotationFill":"#ffffff"},"requirementBackground":"#eee","requirementBorderColor":"hsl(0, 0%, 83.3333333333%)","requirementBorderSize":"1","requirementTextColor":"#111111","relationColor":"#666","relationLabelBackground":"white","relationLabelColor":"#333","git0":"hsl(0, 0%, 70.6862745098%)","git1":"#555","git2":"#BBB","git3":"#777","git4":"#999","git5":"#DDD","git6":"#FFF","git7":"#DDD","gitInv0":"rgb(74.75, 74.75, 74.75)","gitInv1":"#aaaaaa","gitInv2":"#444444","gitInv3":"#888888","gitInv4":"#666666","gitInv5":"#222222","gitInv6":"#000000","gitInv7":"#222222","branchLabelColor":"#333","gitBranchLabel0":"#333","gitBranchLabel1":"white","gitBranchLabel2":"#333","gitBranchLabel3":"white","gitBranchLabel4":"#333","gitBranchLabel5":"#333","gitBranchLabel6":"#333","gitBranchLabel7":"#333","tagLabelColor":"#111111","tagLabelBackground":"#eee","tagLabelBorder":"hsl(0, 0%, 83.3333333333%)","tagLabelFontSize":"10px","commitLabelColor":"rgb(2.7499999999, 2.7499999999, 2.7499999999)","commitLabelBackground":"hsl(0, 0%, 98.9215686275%)","commitLabelFontSize":"10px","emUiFill":"white","emUiStroke":"#dbdada","emProcessorFill":"#edb3f6","emProcessorStroke":"#b88cbf","emReadModelFill":"#d3f1a2","emReadModelStroke":"#a3b732","emCommandFill":"#bcd6fe","emCommandStroke":"#679ac3","emEventFill":"#ffb778","emEventStroke":"#c19a0f","emSwimlaneBackgroundOdd":"rgb(250,250,250)","emSwimlaneBackgroundStroke":"rgb(240,240,240)","emArrowhead":"#666","emRelationStroke":"#666","attributeBackgroundColorOdd":"#ffffff","attributeBackgroundColorEven":"#f2f2f2"}"##;
    const NEUTRAL_BORDER: &str = r##"{"primaryColor":"#eee","contrast":"#707070","secondaryColor":"hsl(0, 0%, 98.9215686275%)","background":"#ffffff","tertiaryColor":"hsl(-160, 0%, 93.3333333333%)","primaryBorderColor":"hsl(0, 0%, 83.3333333333%)","secondaryBorderColor":"hsl(0, 0%, 88.9215686275%)","tertiaryBorderColor":"hsl(-160, 0%, 83.3333333333%)","primaryTextColor":"#111111","secondaryTextColor":"rgb(2.7499999999, 2.7499999999, 2.7499999999)","tertiaryTextColor":"rgb(17.0000000001, 17.0000000001, 17.0000000001)","lineColor":"#666","textColor":"#000000","mainBkg":"#eee","secondBkg":"hsl(0, 0%, 98.9215686275%)","border1":"#999","border2":"#707070","note":"#ffa","text":"#333","critical":"#d42","done":"#bbb","arrowheadColor":"#333333","fontFamily":"Arial","fontSize":"16px","THEME_COLOR_LIMIT":12,"radius":5,"strokeWidth":1,"nodeBkg":"#eee","nodeBorder":"#225577","clusterBkg":"hsl(0, 0%, 98.9215686275%)","clusterBorder":"#707070","defaultLinkColor":"#666","titleColor":"#333","edgeLabelBackground":"white","actorBorder":"hsl(0, 0%, 83%)","actorBkg":"#eee","actorTextColor":"#333","actorLineColor":"hsl(0, 0%, 83%)","signalColor":"#333","signalTextColor":"#333","labelBoxBkgColor":"#eee","labelBoxBorderColor":"hsl(0, 0%, 83%)","labelTextColor":"#333","loopTextColor":"#333","noteBorderColor":"#999","noteBkgColor":"#666","noteTextColor":"#fff","activationBorderColor":"#666","activationBkgColor":"#f4f4f4","sequenceNumberColor":"white","sectionBkgColor":"hsl(0, 0%, 73.9215686275%)","altSectionBkgColor":"white","sectionBkgColor2":"hsl(0, 0%, 73.9215686275%)","excludeBkgColor":"#eeeeee","taskBorderColor":"hsl(0, 0%, 33.9215686275%)","taskBkgColor":"#707070","taskTextLightColor":"white","taskTextColor":"white","taskTextDarkColor":"#333","taskTextOutsideColor":"#333","taskTextClickableColor":"#003163","activeTaskBorderColor":"hsl(0, 0%, 33.9215686275%)","activeTaskBkgColor":"#eee","gridColor":"hsl(0, 0%, 90%)","doneTaskBkgColor":"#bbb","doneTaskBorderColor":"#666","critBkgColor":"#d42","critBorderColor":"hsl(10.9090909091, 73.3333333333%, 40%)","todayLineColor":"#d42","vertLineColor":"#d42","personBorder":"hsl(0, 0%, 83.3333333333%)","personBkg":"#eee","archEdgeColor":"#666","archEdgeArrowColor":"#666","archEdgeWidth":"3","archGroupBorderColor":"hsl(0, 0%, 83.3333333333%)","archGroupBorderWidth":"2px","noteFontWeight":"normal","fontWeight":"normal","rowOdd":"hsl(0, 0%, 100%)","rowEven":"#f4f4f4","labelColor":"black","errorBkgColor":"#552222","errorTextColor":"#552222","useGradient":true,"gradientStart":"hsl(0, 0%, 83.3333333333%)","gradientStop":"hsl(0, 0%, 88.9215686275%)","dropShadow":"drop-shadow( 1px 2px 2px rgba(185,185,185,1))","cScale0":"#123456","rectBkgColor":"hsl(-160, 0%, 93.3333333333%)","cScale1":"#F4F4F4","cScale2":"#555","cScale3":"#BBB","cScale4":"#777","cScale5":"#999","cScale6":"#DDD","cScale7":"#FFF","cScale8":"#DDD","cScale9":"#BBB","cScale10":"#999","cScale11":"#777","cScaleInv0":"#edcba9","cScaleInv1":"#0b0b0b","cScaleInv2":"#aaaaaa","cScaleInv3":"#444444","cScaleInv4":"#888888","cScaleInv5":"#666666","cScaleInv6":"#222222","cScaleInv7":"#000000","cScaleInv8":"#222222","cScaleInv9":"#444444","cScaleInv10":"#666666","cScaleInv11":"#888888","cScalePeer0":"hsl(210, 65.3846153846%, 10.3921568627%)","cScalePeer1":"hsl(0, 0%, 85.6862745098%)","cScalePeer2":"hsl(0, 0%, 23.3333333333%)","cScalePeer3":"hsl(0, 0%, 63.3333333333%)","cScalePeer4":"hsl(0, 0%, 36.6666666667%)","cScalePeer5":"hsl(0, 0%, 50%)","cScalePeer6":"hsl(0, 0%, 76.6666666667%)","cScalePeer7":"hsl(0, 0%, 90%)","cScalePeer8":"hsl(0, 0%, 76.6666666667%)","cScalePeer9":"hsl(0, 0%, 63.3333333333%)","cScalePeer10":"hsl(0, 0%, 50%)","cScalePeer11":"hsl(0, 0%, 36.6666666667%)","scaleLabelColor":"#333","cScaleLabel0":"#F4F4F4","cScaleLabel2":"#F4F4F4","cScaleLabel1":"#333","cScaleLabel3":"#333","cScaleLabel4":"#333","cScaleLabel5":"#333","cScaleLabel6":"#333","cScaleLabel7":"#333","cScaleLabel8":"#333","cScaleLabel9":"#333","cScaleLabel10":"#333","cScaleLabel11":"#333","surface0":"hsl(0, 0%, 88.3333333333%)","surfacePeer0":"hsl(0, 0%, 85.3333333333%)","surface1":"hsl(0, 0%, 83.3333333333%)","surfacePeer1":"hsl(0, 0%, 80.3333333333%)","surface2":"hsl(0, 0%, 78.3333333333%)","surfacePeer2":"hsl(0, 0%, 75.3333333333%)","surface3":"hsl(0, 0%, 73.3333333333%)","surfacePeer3":"hsl(0, 0%, 70.3333333333%)","surface4":"hsl(0, 0%, 68.3333333333%)","surfacePeer4":"hsl(0, 0%, 65.3333333333%)","flowContainerStroke":"hsl(0, 0%, 88.9215686275%)","transitionColor":"#000","transitionLabelColor":"#000000","stateLabelColor":"#111111","stateBkg":"#eee","labelBackgroundColor":"#eee","compositeBackground":"#ffffff","altBackground":"#f4f4f4","compositeTitleBackground":"#eee","stateBorder":"#000","innerEndBackground":"hsl(0, 0%, 83.3333333333%)","specialStateColor":"#222","classText":"#111111","fillType0":"#eee","fillType1":"hsl(0, 0%, 98.9215686275%)","fillType2":"hsl(64, 0%, 93.3333333333%)","fillType3":"hsl(64, 0%, 98.9215686275%)","fillType4":"hsl(-64, 0%, 93.3333333333%)","fillType5":"hsl(-64, 0%, 98.9215686275%)","fillType6":"hsl(128, 0%, 93.3333333333%)","fillType7":"hsl(128, 0%, 98.9215686275%)","pie0":"#123456","pie1":"#F4F4F4","pie2":"#555","pie3":"#BBB","pie4":"#777","pie5":"#999","pie6":"#DDD","pie7":"#FFF","pie8":"#DDD","pie9":"#BBB","pie10":"#999","pie11":"#777","pie12":"#123456","pieTitleTextSize":"25px","pieTitleTextColor":"#333","pieSectionTextSize":"17px","pieSectionTextColor":"#000000","pieLegendTextSize":"17px","pieLegendTextColor":"#333","pieStrokeColor":"black","pieStrokeWidth":"2px","pieOuterStrokeWidth":"2px","pieOuterStrokeColor":"black","pieOpacity":"0.7","venn1":"#123456","venn2":"#F4F4F4","venn3":"#555","venn4":"#BBB","venn5":"#777","venn6":"#999","venn7":"#DDD","venn8":"#FFF","vennTitleTextColor":"#333","vennSetTextColor":"#000000","cynefin":{"domainFontSize":16,"itemFontSize":12,"boundaryColor":"#666","boundaryWidth":2,"cliffColor":"#8B0000","cliffWidth":4,"arrowColor":"#666","arrowWidth":2,"complexBg":"#E8F5E9","complicatedBg":"#E3F2FD","chaoticBg":"#FBE9E7","clearBg":"#FFF8E1","confusionBg":"#F3E5F5","textColor":"#000000","labelColor":"#111111"},"quadrant1Fill":"#eee","quadrant2Fill":"#f3f3f3","quadrant3Fill":"#f8f8f8","quadrant4Fill":"#fdfdfd","quadrant1TextFill":"#111111","quadrant2TextFill":"#0c0c0c","quadrant3TextFill":"#070707","quadrant4TextFill":"#020202","quadrantPointFill":"hsl(0, 0%, NaN%)","quadrantPointTextFill":"#111111","quadrantXAxisTextFill":"#111111","quadrantYAxisTextFill":"#111111","quadrantInternalBorderStrokeFill":"hsl(0, 0%, 83.3333333333%)","quadrantExternalBorderStrokeFill":"hsl(0, 0%, 83.3333333333%)","quadrantTitleFill":"#111111","xyChart":{"backgroundColor":"#ffffff","titleColor":"#111111","dataLabelColor":"#111111","legendTextColor":"#111111","xAxisTitleColor":"#111111","xAxisLabelColor":"#111111","xAxisTickColor":"#111111","xAxisLineColor":"#111111","yAxisTitleColor":"#111111","yAxisLabelColor":"#111111","yAxisTickColor":"#111111","yAxisLineColor":"#111111","plotColorPalette":"#EEE,#6BB8E4,#8ACB88,#C7ACD6,#E8DCC2,#FFB2A8,#FFF380,#7E8D91,#FFD8B1,#FAF3E0"},"radar":{"axisColor":"#666","axisStrokeWidth":2,"axisLabelFontSize":12,"curveOpacity":0.5,"curveStrokeWidth":2,"graticuleColor":"#DEDEDE","graticuleStrokeWidth":1,"graticuleOpacity":0.3,"legendBoxSize":12,"legendFontSize":12},"wardleyEvolutionColor":"#dc3545","wardley":{"backgroundColor":"#ffffff","axisColor":"#666","axisTextColor":"#111111","gridColor":"hsl(0, 0%, 90%)","componentFill":"#ffffff","componentStroke":"#666","componentLabelColor":"#111111","linkStroke":"#666","evolutionStroke":"#dc3545","annotationStroke":"#666","annotationTextColor":"#111111","annotationFill":"#ffffff"},"requirementBackground":"#eee","requirementBorderColor":"hsl(0, 0%, 83.3333333333%)","requirementBorderSize":"1","requirementTextColor":"#111111","relationColor":"#666","relationLabelBackground":"white","relationLabelColor":"#333","git0":"hsl(0, 0%, 70.6862745098%)","git1":"#555","git2":"#BBB","git3":"#777","git4":"#999","git5":"#DDD","git6":"#FFF","git7":"#DDD","gitInv0":"rgb(74.75, 74.75, 74.75)","gitInv1":"#aaaaaa","gitInv2":"#444444","gitInv3":"#888888","gitInv4":"#666666","gitInv5":"#222222","gitInv6":"#000000","gitInv7":"#222222","branchLabelColor":"#333","gitBranchLabel0":"#333","gitBranchLabel1":"white","gitBranchLabel2":"#333","gitBranchLabel3":"white","gitBranchLabel4":"#333","gitBranchLabel5":"#333","gitBranchLabel6":"#333","gitBranchLabel7":"#333","tagLabelColor":"#111111","tagLabelBackground":"#eee","tagLabelBorder":"hsl(0, 0%, 83.3333333333%)","tagLabelFontSize":"10px","commitLabelColor":"rgb(2.7499999999, 2.7499999999, 2.7499999999)","commitLabelBackground":"hsl(0, 0%, 98.9215686275%)","commitLabelFontSize":"10px","emUiFill":"white","emUiStroke":"#dbdada","emProcessorFill":"#edb3f6","emProcessorStroke":"#b88cbf","emReadModelFill":"#d3f1a2","emReadModelStroke":"#a3b732","emCommandFill":"#bcd6fe","emCommandStroke":"#679ac3","emEventFill":"#ffb778","emEventStroke":"#c19a0f","emSwimlaneBackgroundOdd":"rgb(250,250,250)","emSwimlaneBackgroundStroke":"rgb(240,240,240)","emArrowhead":"#666","emRelationStroke":"#666","attributeBackgroundColorOdd":"#ffffff","attributeBackgroundColorEven":"#f2f2f2"}"##;
    const BASE_NONE: &str = r##"{"background":"#f4f4f4","primaryColor":"#fff4dd","noteBkgColor":"#fff5ad","noteTextColor":"#333","THEME_COLOR_LIMIT":12,"radius":5,"strokeWidth":1,"fontFamily":"\"trebuchet ms\", verdana, arial, sans-serif","fontSize":"16px","useGradient":true,"dropShadow":"drop-shadow( 1px 2px 2px rgba(185,185,185,1))","primaryTextColor":"#333","secondaryColor":"hsl(-79.4117647059, 100%, 93.3333333333%)","tertiaryColor":"hsl(220.5882352941, 100%, 98.3333333333%)","primaryBorderColor":"hsl(40.5882352941, 60%, 83.3333333333%)","secondaryBorderColor":"hsl(-79.4117647059, 60%, 83.3333333333%)","tertiaryBorderColor":"hsl(220.5882352941, 60%, 88.3333333333%)","noteBorderColor":"hsl(52.6829268293, 60%, 73.9215686275%)","secondaryTextColor":"rgb(11.0000000001, 34.0000000002, 0)","tertiaryTextColor":"rgb(8.5000000002, 5.7500000001, 0)","lineColor":"#0b0b0b","arrowheadColor":"#0b0b0b","textColor":"#333","border2":"hsl(220.5882352941, 60%, 88.3333333333%)","nodeBkg":"#fff4dd","mainBkg":"#fff4dd","nodeBorder":"hsl(40.5882352941, 60%, 83.3333333333%)","clusterBkg":"hsl(220.5882352941, 100%, 98.3333333333%)","clusterBorder":"hsl(220.5882352941, 60%, 88.3333333333%)","defaultLinkColor":"#0b0b0b","titleColor":"rgb(8.5000000002, 5.7500000001, 0)","edgeLabelBackground":"hsl(-79.4117647059, 100%, 93.3333333333%)","flowContainerStroke":"hsl(-79.4117647059, 60%, 83.3333333333%)","nodeTextColor":"#333","actorBorder":"hsl(40.5882352941, 60%, 83.3333333333%)","actorBkg":"#fff4dd","actorTextColor":"#333","actorLineColor":"hsl(40.5882352941, 60%, 83.3333333333%)","labelBoxBkgColor":"#fff4dd","signalColor":"#333","signalTextColor":"#333","labelBoxBorderColor":"hsl(40.5882352941, 60%, 83.3333333333%)","labelTextColor":"#333","loopTextColor":"#333","activationBorderColor":"hsl(-79.4117647059, 100%, 83.3333333333%)","activationBkgColor":"hsl(-79.4117647059, 100%, 93.3333333333%)","sequenceNumberColor":"#f4f4f4","rectBkgColor":"hsl(220.5882352941, 100%, 98.3333333333%)","sectionBkgColor":"hsl(220.5882352941, 100%, 98.3333333333%)","altSectionBkgColor":"white","sectionBkgColor2":"#fff4dd","excludeBkgColor":"#eeeeee","taskBorderColor":"hsl(40.5882352941, 60%, 83.3333333333%)","taskBkgColor":"#fff4dd","activeTaskBorderColor":"#fff4dd","activeTaskBkgColor":"hsl(40.5882352941, 100%, 100%)","gridColor":"lightgrey","doneTaskBkgColor":"lightgrey","doneTaskBorderColor":"grey","critBorderColor":"#ff8888","critBkgColor":"red","todayLineColor":"red","vertLineColor":"navy","taskTextColor":"#333","taskTextOutsideColor":"#333","taskTextLightColor":"#333","taskTextDarkColor":"#333","taskTextClickableColor":"#003163","noteFontWeight":"normal","fontWeight":"normal","personBorder":"hsl(40.5882352941, 60%, 83.3333333333%)","personBkg":"#fff4dd","rowOdd":"hsl(40.5882352941, 100%, 100%)","rowEven":"hsl(40.5882352941, 100%, 98.3333333333%)","transitionColor":"#0b0b0b","transitionLabelColor":"#333","stateLabelColor":"#333","stateBkg":"#fff4dd","labelBackgroundColor":"#fff4dd","compositeBackground":"#f4f4f4","altBackground":"hsl(220.5882352941, 100%, 98.3333333333%)","compositeTitleBackground":"#fff4dd","compositeBorder":"hsl(40.5882352941, 60%, 83.3333333333%)","innerEndBackground":"hsl(40.5882352941, 60%, 83.3333333333%)","errorBkgColor":"hsl(220.5882352941, 100%, 98.3333333333%)","errorTextColor":"rgb(8.5000000002, 5.7500000001, 0)","specialStateColor":"#0b0b0b","cScale0":"hsl(40.5882352941, 100%, 68.3333333333%)","cScale1":"hsl(-79.4117647059, 100%, 68.3333333333%)","cScale2":"hsl(220.5882352941, 100%, 73.3333333333%)","cScale3":"hsl(70.5882352941, 100%, 68.3333333333%)","cScale4":"hsl(100.5882352941, 100%, 68.3333333333%)","cScale5":"hsl(130.5882352941, 100%, 68.3333333333%)","cScale6":"hsl(160.5882352941, 100%, 68.3333333333%)","cScale7":"hsl(190.5882352941, 100%, 68.3333333333%)","cScale8":"hsl(250.5882352941, 100%, 75%)","cScale9":"hsl(310.5882352941, 100%, 68.3333333333%)","cScale10":"hsl(340.5882352941, 100%, 68.3333333333%)","cScale11":"hsl(10.5882352941, 100%, 68.3333333333%)","cScaleInv0":"rgb(0, 52.2500000001, 161.5000000002)","cScaleInv1":"rgb(52.2500000001, 161.5000000002, 0)","cScaleInv2":"rgb(136.0000000002, 92.0000000001, 0)","cScaleInv3":"rgb(28.5, 0, 161.5000000002)","cScaleInv4":"rgb(109.2500000001, 0, 161.5000000002)","cScaleInv5":"rgb(161.5000000002, 0, 133.0000000002)","cScaleInv6":"rgb(161.5000000002, 0, 52.2500000001)","cScaleInv7":"rgb(161.5000000002, 28.5, 0)","cScaleInv8":"rgb(105, 127.5, 0)","cScaleInv9":"rgb(0, 161.5000000002, 28.5)","cScaleInv10":"rgb(0, 161.5000000002, 109.2500000001)","cScaleInv11":"rgb(0, 133.0000000002, 161.5000000002)","cScalePeer0":"hsl(40.5882352941, 100%, 58.3333333333%)","cScalePeer1":"hsl(-79.4117647059, 100%, 58.3333333333%)","cScalePeer2":"hsl(220.5882352941, 100%, 63.3333333333%)","cScalePeer3":"hsl(70.5882352941, 100%, 58.3333333333%)","cScalePeer4":"hsl(100.5882352941, 100%, 58.3333333333%)","cScalePeer5":"hsl(130.5882352941, 100%, 58.3333333333%)","cScalePeer6":"hsl(160.5882352941, 100%, 58.3333333333%)","cScalePeer7":"hsl(190.5882352941, 100%, 58.3333333333%)","cScalePeer8":"hsl(250.5882352941, 100%, 65%)","cScalePeer9":"hsl(310.5882352941, 100%, 58.3333333333%)","cScalePeer10":"hsl(340.5882352941, 100%, 58.3333333333%)","cScalePeer11":"hsl(10.5882352941, 100%, 58.3333333333%)","scaleLabelColor":"#333","cScaleLabel0":"#333","cScaleLabel1":"#333","cScaleLabel2":"#333","cScaleLabel3":"#333","cScaleLabel4":"#333","cScaleLabel5":"#333","cScaleLabel6":"#333","cScaleLabel7":"#333","cScaleLabel8":"#333","cScaleLabel9":"#333","cScaleLabel10":"#333","cScaleLabel11":"#333","surface0":"hsl(220.5882352941, 85%, 88.3333333333%)","surfacePeer0":"hsl(220.5882352941, 85%, 85.3333333333%)","surface1":"hsl(220.5882352941, 85%, 85.3333333333%)","surfacePeer1":"hsl(220.5882352941, 85%, 82.3333333333%)","surface2":"hsl(220.5882352941, 85%, 82.3333333333%)","surfacePeer2":"hsl(220.5882352941, 85%, 79.3333333333%)","surface3":"hsl(220.5882352941, 85%, 79.3333333333%)","surfacePeer3":"hsl(220.5882352941, 85%, 76.3333333333%)","surface4":"hsl(220.5882352941, 85%, 76.3333333333%)","surfacePeer4":"hsl(220.5882352941, 85%, 73.3333333333%)","classText":"#333","fillType0":"#fff4dd","fillType1":"hsl(-79.4117647059, 100%, 93.3333333333%)","fillType2":"hsl(104.5882352941, 100%, 93.3333333333%)","fillType3":"hsl(-15.4117647059, 100%, 93.3333333333%)","fillType4":"hsl(-23.4117647059, 100%, 93.3333333333%)","fillType5":"hsl(-143.4117647059, 100%, 93.3333333333%)","fillType6":"hsl(168.5882352941, 100%, 93.3333333333%)","fillType7":"hsl(48.5882352941, 100%, 93.3333333333%)","pie1":"#fff4dd","pie2":"hsl(-79.4117647059, 100%, 93.3333333333%)","pie3":"hsl(220.5882352941, 100%, 98.3333333333%)","pie4":"hsl(40.5882352941, 100%, 83.3333333333%)","pie5":"hsl(-79.4117647059, 100%, 83.3333333333%)","pie6":"hsl(220.5882352941, 100%, 88.3333333333%)","pie7":"hsl(100.5882352941, 100%, 83.3333333333%)","pie8":"hsl(-19.4117647059, 100%, 83.3333333333%)","pie9":"hsl(160.5882352941, 100%, 93.3333333333%)","pie10":"hsl(100.5882352941, 100%, 73.3333333333%)","pie11":"hsl(-19.4117647059, 100%, 73.3333333333%)","pie12":"hsl(160.5882352941, 100%, 83.3333333333%)","pieTitleTextSize":"25px","pieTitleTextColor":"#333","pieSectionTextSize":"17px","pieSectionTextColor":"#333","pieLegendTextSize":"17px","pieLegendTextColor":"#333","pieStrokeColor":"black","pieStrokeWidth":"2px","pieOuterStrokeWidth":"2px","pieOuterStrokeColor":"black","pieOpacity":"0.7","venn1":"hsl(40.5882352941, 100%, 63.3333333333%)","venn2":"hsl(-79.4117647059, 100%, 63.3333333333%)","venn3":"hsl(220.5882352941, 100%, 68.3333333333%)","venn4":"hsl(100.5882352941, 100%, 63.3333333333%)","venn5":"hsl(-19.4117647059, 100%, 63.3333333333%)","venn6":"hsl(-19.4117647059, 100%, 63.3333333333%)","venn7":"hsl(160.5882352941, 100%, 63.3333333333%)","venn8":"hsl(40.5882352941, 100%, 63.3333333333%)","vennTitleTextColor":"rgb(8.5000000002, 5.7500000001, 0)","vennSetTextColor":"#333","cynefin":{"domainFontSize":16,"itemFontSize":12,"boundaryColor":"#0b0b0b","boundaryWidth":2,"cliffColor":"#8B0000","cliffWidth":4,"arrowColor":"#0b0b0b","arrowWidth":2,"complexBg":"#E8F5E9","complicatedBg":"#E3F2FD","chaoticBg":"#FBE9E7","clearBg":"#FFF8E1","confusionBg":"#F3E5F5","textColor":"#333","labelColor":"#333"},"radar":{"axisColor":"#0b0b0b","axisStrokeWidth":2,"axisLabelFontSize":12,"curveOpacity":0.5,"curveStrokeWidth":2,"graticuleColor":"#DEDEDE","graticuleStrokeWidth":1,"graticuleOpacity":0.3,"legendBoxSize":12,"legendFontSize":12},"wardleyEvolutionColor":"#dc3545","wardley":{"backgroundColor":"#f4f4f4","axisColor":"#0b0b0b","axisTextColor":"#333","gridColor":"lightgrey","componentFill":"#f4f4f4","componentStroke":"#0b0b0b","componentLabelColor":"#333","linkStroke":"#0b0b0b","evolutionStroke":"#dc3545","annotationStroke":"#0b0b0b","annotationTextColor":"#333","annotationFill":"#f4f4f4"},"archEdgeColor":"#777","archEdgeArrowColor":"#777","archEdgeWidth":"3","archGroupBorderColor":"#000","archGroupBorderWidth":"2px","quadrant1Fill":"#fff4dd","quadrant2Fill":"#fff9e2","quadrant3Fill":"#fffee7","quadrant4Fill":"#ffffec","quadrant1TextFill":"#333","quadrant2TextFill":"#2e2e2e","quadrant3TextFill":"#292929","quadrant4TextFill":"#242424","quadrantPointFill":"hsl(40.5882352941, 100%, NaN%)","quadrantPointTextFill":"#333","quadrantXAxisTextFill":"#333","quadrantYAxisTextFill":"#333","quadrantInternalBorderStrokeFill":"hsl(40.5882352941, 60%, 83.3333333333%)","quadrantExternalBorderStrokeFill":"hsl(40.5882352941, 60%, 83.3333333333%)","quadrantTitleFill":"#333","xyChart":{"backgroundColor":"#f4f4f4","titleColor":"#333","dataLabelColor":"#333","legendTextColor":"#333","xAxisTitleColor":"#333","xAxisLabelColor":"#333","xAxisTickColor":"#333","xAxisLineColor":"#333","yAxisTitleColor":"#333","yAxisLabelColor":"#333","yAxisTickColor":"#333","yAxisLineColor":"#333","plotColorPalette":"#FFF4DD,#FFD8B1,#FFA07A,#ECEFF1,#D6DBDF,#C3E0A8,#FFB6A4,#FFD74D,#738FA7,#FFFFF0"},"requirementBackground":"#fff4dd","requirementBorderColor":"hsl(40.5882352941, 60%, 83.3333333333%)","requirementBorderSize":"1","requirementTextColor":"#333","relationColor":"#0b0b0b","relationLabelBackground":"hsl(-79.4117647059, 100%, 93.3333333333%)","relationLabelColor":"#333","git0":"hsl(40.5882352941, 100%, 68.3333333333%)","git1":"hsl(-79.4117647059, 100%, 68.3333333333%)","git2":"hsl(220.5882352941, 100%, 73.3333333333%)","git3":"hsl(10.5882352941, 100%, 68.3333333333%)","git4":"hsl(-19.4117647059, 100%, 68.3333333333%)","git5":"hsl(-49.4117647059, 100%, 68.3333333333%)","git6":"hsl(100.5882352941, 100%, 68.3333333333%)","git7":"hsl(160.5882352941, 100%, 68.3333333333%)","gitInv0":"rgb(0, 52.2500000001, 161.5000000002)","gitInv1":"rgb(52.2500000001, 161.5000000002, 0)","gitInv2":"rgb(136.0000000002, 92.0000000001, 0)","gitInv3":"rgb(0, 133.0000000002, 161.5000000002)","gitInv4":"rgb(0, 161.5000000002, 109.2500000001)","gitInv5":"rgb(0, 161.5000000002, 28.5)","gitInv6":"rgb(109.2500000001, 0, 161.5000000002)","gitInv7":"rgb(161.5000000002, 0, 52.2500000001)","branchLabelColor":"#333","gitBranchLabel0":"#333","gitBranchLabel1":"#333","gitBranchLabel2":"#333","gitBranchLabel3":"#333","gitBranchLabel4":"#333","gitBranchLabel5":"#333","gitBranchLabel6":"#333","gitBranchLabel7":"#333","tagLabelColor":"#333","tagLabelBackground":"#fff4dd","tagLabelBorder":"hsl(40.5882352941, 60%, 83.3333333333%)","tagLabelFontSize":"10px","commitLabelColor":"rgb(11.0000000001, 34.0000000002, 0)","commitLabelBackground":"hsl(-79.4117647059, 100%, 93.3333333333%)","commitLabelFontSize":"10px","emUiFill":"white","emUiStroke":"#dbdada","emProcessorFill":"#edb3f6","emProcessorStroke":"#b88cbf","emReadModelFill":"#d3f1a2","emReadModelStroke":"#a3b732","emCommandFill":"#bcd6fe","emCommandStroke":"#679ac3","emEventFill":"#ffb778","emEventStroke":"#c19a0f","emSwimlaneBackgroundOdd":"rgb(250,250,250)","emSwimlaneBackgroundStroke":"rgb(240,240,240)","emArrowhead":"#0b0b0b","emRelationStroke":"#0b0b0b","attributeBackgroundColorOdd":"#ffffff","attributeBackgroundColorEven":"#f2f2f2","gradientStart":"hsl(40.5882352941, 60%, 83.3333333333%)","gradientStop":"hsl(-79.4117647059, 60%, 83.3333333333%)"}"##;
    const BASE_COLORS: &str = r##"{"background":"#f4f4f4","primaryColor":"#abcdef","noteBkgColor":"#fff5ad","noteTextColor":"#333","THEME_COLOR_LIMIT":12,"radius":5,"strokeWidth":1,"fontFamily":"\"trebuchet ms\", verdana, arial, sans-serif","fontSize":"16px","useGradient":true,"dropShadow":"drop-shadow( 1px 2px 2px rgba(185,185,185,1))","secondaryColor":"#123456","darkMode":true,"primaryTextColor":"#eee","tertiaryColor":"hsl(30, 68%, 85.3921568627%)","primaryBorderColor":"hsl(210, 28%, 90.3921568627%)","secondaryBorderColor":"hsl(210, 25.3846153846%, 30.3921568627%)","tertiaryBorderColor":"hsl(30, 28%, 95.3921568627%)","noteBorderColor":"hsl(52.6829268293, 60%, 93.9215686275%)","secondaryTextColor":"#edcba9","tertiaryTextColor":"rgb(11.92, 37.2500000001, 62.5800000002)","lineColor":"#0b0b0b","arrowheadColor":"#0b0b0b","textColor":"#eee","border2":"hsl(30, 28%, 95.3921568627%)","nodeBkg":"#abcdef","mainBkg":"#abcdef","nodeBorder":"hsl(210, 28%, 90.3921568627%)","clusterBkg":"hsl(30, 68%, 85.3921568627%)","clusterBorder":"hsl(30, 28%, 95.3921568627%)","defaultLinkColor":"#0b0b0b","titleColor":"rgb(11.92, 37.2500000001, 62.5800000002)","edgeLabelBackground":"hsl(210, 65.3846153846%, 0%)","flowContainerStroke":"hsl(210, 25.3846153846%, 30.3921568627%)","nodeTextColor":"#eee","actorBorder":"hsl(210, 28%, 90.3921568627%)","actorBkg":"#abcdef","actorTextColor":"#eee","actorLineColor":"hsl(210, 28%, 90.3921568627%)","labelBoxBkgColor":"#abcdef","signalColor":"#eee","signalTextColor":"#eee","labelBoxBorderColor":"hsl(210, 28%, 90.3921568627%)","labelTextColor":"#eee","loopTextColor":"#eee","activationBorderColor":"hsl(210, 65.3846153846%, 10.3921568627%)","activationBkgColor":"#123456","sequenceNumberColor":"#f4f4f4","rectBkgColor":"hsl(30, 68%, 85.3921568627%)","sectionBkgColor":"hsl(30, 68%, 85.3921568627%)","altSectionBkgColor":"white","sectionBkgColor2":"#abcdef","excludeBkgColor":"#eeeeee","taskBorderColor":"hsl(210, 28%, 90.3921568627%)","taskBkgColor":"#abcdef","activeTaskBorderColor":"#abcdef","activeTaskBkgColor":"hsl(210, 68%, 100%)","gridColor":"lightgrey","doneTaskBkgColor":"lightgrey","doneTaskBorderColor":"grey","critBorderColor":"#ff8888","critBkgColor":"red","todayLineColor":"red","vertLineColor":"navy","taskTextColor":"#eee","taskTextOutsideColor":"#eee","taskTextLightColor":"#eee","taskTextDarkColor":"#eee","taskTextClickableColor":"#003163","noteFontWeight":"normal","fontWeight":"normal","personBorder":"hsl(210, 28%, 90.3921568627%)","personBkg":"#abcdef","rowOdd":"hsl(210, 68%, 75.3921568627%)","rowEven":"hsl(210, 68%, 70.3921568627%)","transitionColor":"#0b0b0b","transitionLabelColor":"#eee","stateLabelColor":"#eee","stateBkg":"#abcdef","labelBackgroundColor":"#abcdef","compositeBackground":"#f4f4f4","altBackground":"hsl(30, 68%, 85.3921568627%)","compositeTitleBackground":"#abcdef","compositeBorder":"hsl(210, 28%, 90.3921568627%)","innerEndBackground":"hsl(210, 28%, 90.3921568627%)","errorBkgColor":"hsl(30, 68%, 85.3921568627%)","errorTextColor":"rgb(11.92, 37.2500000001, 62.5800000002)","specialStateColor":"#0b0b0b","cScale0":"hsl(210, 68%, 5.3921568627%)","cScale1":"hsl(210, 65.3846153846%, 0%)","cScale2":"hsl(30, 68%, 10.3921568627%)","cScale3":"hsl(240, 68%, 5.3921568627%)","cScale4":"hsl(270, 68%, 5.3921568627%)","cScale5":"hsl(300, 68%, 5.3921568627%)","cScale6":"hsl(330, 68%, 5.3921568627%)","cScale7":"hsl(0, 68%, 5.3921568627%)","cScale8":"hsl(60, 68%, 25%)","cScale9":"hsl(120, 68%, 5.3921568627%)","cScale10":"hsl(150, 68%, 5.3921568627%)","cScale11":"hsl(180, 68%, 5.3921568627%)","cScaleInv0":"rgb(250.6, 241.2500000001, 231.9000000002)","cScaleInv1":"#ffffff","cScaleInv2":"rgb(210.4800000002, 228.5000000001, 246.52)","cScaleInv3":"rgb(250.6, 250.6, 231.9000000002)","cScaleInv4":"rgb(241.2500000001, 250.6, 231.9000000002)","cScaleInv5":"rgb(231.9000000002, 250.6, 231.9000000002)","cScaleInv6":"rgb(231.9000000002, 250.6, 241.2500000001)","cScaleInv7":"rgb(231.9000000002, 250.6, 250.6)","cScaleInv8":"rgb(147.9, 147.9, 234.6)","cScaleInv9":"rgb(250.6, 231.9000000002, 250.6)","cScaleInv10":"rgb(250.6, 231.9000000002, 241.2500000001)","cScaleInv11":"rgb(250.6, 231.9000000002, 231.9000000002)","cScalePeer0":"hsl(210, 68%, 15.3921568627%)","cScalePeer1":"hsl(210, 65.3846153846%, 10%)","cScalePeer2":"hsl(30, 68%, 20.3921568627%)","cScalePeer3":"hsl(240, 68%, 15.3921568627%)","cScalePeer4":"hsl(270, 68%, 15.3921568627%)","cScalePeer5":"hsl(300, 68%, 15.3921568627%)","cScalePeer6":"hsl(330, 68%, 15.3921568627%)","cScalePeer7":"hsl(0, 68%, 15.3921568627%)","cScalePeer8":"hsl(60, 68%, 35%)","cScalePeer9":"hsl(120, 68%, 15.3921568627%)","cScalePeer10":"hsl(150, 68%, 15.3921568627%)","cScalePeer11":"hsl(180, 68%, 15.3921568627%)","scaleLabelColor":"#eee","cScaleLabel0":"#eee","cScaleLabel1":"#eee","cScaleLabel2":"#eee","cScaleLabel3":"#eee","cScaleLabel4":"#eee","cScaleLabel5":"#eee","cScaleLabel6":"#eee","cScaleLabel7":"#eee","cScaleLabel8":"#eee","cScaleLabel9":"#eee","cScaleLabel10":"#eee","cScaleLabel11":"#eee","surface0":"hsl(30, 53%, 60.3921568627%)","surfacePeer0":"hsl(30, 53%, 48.3921568627%)","surface1":"hsl(30, 53%, 48.3921568627%)","surfacePeer1":"hsl(30, 53%, 36.3921568627%)","surface2":"hsl(30, 53%, 36.3921568627%)","surfacePeer2":"hsl(30, 53%, 24.3921568627%)","surface3":"hsl(30, 53%, 24.3921568627%)","surfacePeer3":"hsl(30, 53%, 12.3921568627%)","surface4":"hsl(30, 53%, 12.3921568627%)","surfacePeer4":"hsl(30, 53%, 0.3921568627%)","classText":"#eee","fillType0":"#abcdef","fillType1":"#123456","fillType2":"hsl(274, 68%, 80.3921568627%)","fillType3":"hsl(274, 65.3846153846%, 20.3921568627%)","fillType4":"hsl(146, 68%, 80.3921568627%)","fillType5":"hsl(146, 65.3846153846%, 20.3921568627%)","fillType6":"hsl(338, 68%, 80.3921568627%)","fillType7":"hsl(338, 65.3846153846%, 20.3921568627%)","pie1":"#abcdef","pie2":"#123456","pie3":"hsl(30, 68%, 85.3921568627%)","pie4":"hsl(210, 68%, 70.3921568627%)","pie5":"hsl(210, 65.3846153846%, 10.3921568627%)","pie6":"hsl(30, 68%, 75.3921568627%)","pie7":"hsl(270, 68%, 70.3921568627%)","pie8":"hsl(150, 68%, 70.3921568627%)","pie9":"hsl(330, 68%, 80.3921568627%)","pie10":"hsl(270, 68%, 60.3921568627%)","pie11":"hsl(150, 68%, 60.3921568627%)","pie12":"hsl(330, 68%, 70.3921568627%)","pieTitleTextSize":"25px","pieTitleTextColor":"#eee","pieSectionTextSize":"17px","pieSectionTextColor":"#eee","pieLegendTextSize":"17px","pieLegendTextColor":"#eee","pieStrokeColor":"black","pieStrokeWidth":"2px","pieOuterStrokeWidth":"2px","pieOuterStrokeColor":"black","pieOpacity":"0.7","venn1":"hsl(210, 68%, 50.3921568627%)","venn2":"hsl(210, 65.3846153846%, 0%)","venn3":"hsl(30, 68%, 55.3921568627%)","venn4":"hsl(270, 68%, 50.3921568627%)","venn5":"hsl(150, 68%, 50.3921568627%)","venn6":"hsl(270, 65.3846153846%, 0%)","venn7":"hsl(330, 68%, 50.3921568627%)","venn8":"hsl(330, 65.3846153846%, 0%)","vennTitleTextColor":"rgb(11.92, 37.2500000001, 62.5800000002)","vennSetTextColor":"#eee","cynefin":{"domainFontSize":16,"itemFontSize":12,"boundaryColor":"#0b0b0b","boundaryWidth":2,"cliffColor":"#8B0000","cliffWidth":4,"arrowColor":"#0b0b0b","arrowWidth":2,"complexBg":"#E8F5E9","complicatedBg":"#E3F2FD","chaoticBg":"#FBE9E7","clearBg":"#FFF8E1","confusionBg":"#F3E5F5","textColor":"#eee","labelColor":"#eee"},"radar":{"axisColor":"#0b0b0b","axisStrokeWidth":2,"axisLabelFontSize":12,"curveOpacity":0.5,"curveStrokeWidth":2,"graticuleColor":"#DEDEDE","graticuleStrokeWidth":1,"graticuleOpacity":0.3,"legendBoxSize":12,"legendFontSize":12},"wardleyEvolutionColor":"#dc3545","wardley":{"backgroundColor":"#f4f4f4","axisColor":"#0b0b0b","axisTextColor":"#eee","gridColor":"lightgrey","componentFill":"#f4f4f4","componentStroke":"#0b0b0b","componentLabelColor":"#eee","linkStroke":"#0b0b0b","evolutionStroke":"#dc3545","annotationStroke":"#0b0b0b","annotationTextColor":"#eee","annotationFill":"#f4f4f4"},"archEdgeColor":"#777","archEdgeArrowColor":"#777","archEdgeWidth":"3","archGroupBorderColor":"#000","archGroupBorderWidth":"2px","quadrant1Fill":"#abcdef","quadrant2Fill":"#b0d2f4","quadrant3Fill":"#b5d7f9","quadrant4Fill":"#badcfe","quadrant1TextFill":"#eee","quadrant2TextFill":"#e9e9e9","quadrant3TextFill":"#e4e4e4","quadrant4TextFill":"#dfdfdf","quadrantPointFill":"hsl(210, 68%, NaN%)","quadrantPointTextFill":"#eee","quadrantXAxisTextFill":"#eee","quadrantYAxisTextFill":"#eee","quadrantInternalBorderStrokeFill":"hsl(210, 28%, 90.3921568627%)","quadrantExternalBorderStrokeFill":"hsl(210, 28%, 90.3921568627%)","quadrantTitleFill":"#eee","xyChart":{"backgroundColor":"#f4f4f4","titleColor":"#eee","dataLabelColor":"#eee","legendTextColor":"#eee","xAxisTitleColor":"#eee","xAxisLabelColor":"#eee","xAxisTickColor":"#eee","xAxisLineColor":"#eee","yAxisTitleColor":"#eee","yAxisLabelColor":"#eee","yAxisTickColor":"#eee","yAxisLineColor":"#eee","plotColorPalette":"#FFF4DD,#FFD8B1,#FFA07A,#ECEFF1,#D6DBDF,#C3E0A8,#FFB6A4,#FFD74D,#738FA7,#FFFFF0"},"requirementBackground":"#abcdef","requirementBorderColor":"hsl(210, 28%, 90.3921568627%)","requirementBorderSize":"1","requirementTextColor":"#eee","relationColor":"#0b0b0b","relationLabelBackground":"hsl(210, 65.3846153846%, 0%)","relationLabelColor":"#eee","git0":"hsl(210, 68%, 100%)","git1":"hsl(210, 65.3846153846%, 45.3921568627%)","git2":"hsl(30, 68%, 100%)","git3":"hsl(180, 68%, 100%)","git4":"hsl(150, 68%, 100%)","git5":"hsl(120, 68%, 100%)","git6":"hsl(270, 68%, 100%)","git7":"hsl(330, 68%, 100%)","gitInv0":"#000000","gitInv1":"rgb(214.9326923077, 139.2500000001, 63.5673076925)","gitInv2":"#000000","gitInv3":"#000000","gitInv4":"#000000","gitInv5":"#000000","gitInv6":"#000000","gitInv7":"#000000","branchLabelColor":"black","gitBranchLabel0":"black","gitBranchLabel1":"black","gitBranchLabel2":"black","gitBranchLabel3":"black","gitBranchLabel4":"black","gitBranchLabel5":"black","gitBranchLabel6":"black","gitBranchLabel7":"black","tagLabelColor":"#eee","tagLabelBackground":"#abcdef","tagLabelBorder":"hsl(210, 28%, 90.3921568627%)","tagLabelFontSize":"10px","commitLabelColor":"#edcba9","commitLabelBackground":"#123456","commitLabelFontSize":"10px","emUiFill":"white","emUiStroke":"#dbdada","emProcessorFill":"#edb3f6","emProcessorStroke":"#b88cbf","emReadModelFill":"#d3f1a2","emReadModelStroke":"#a3b732","emCommandFill":"#bcd6fe","emCommandStroke":"#679ac3","emEventFill":"#ffb778","emEventStroke":"#c19a0f","emSwimlaneBackgroundOdd":"rgb(250,250,250)","emSwimlaneBackgroundStroke":"rgb(240,240,240)","emArrowhead":"#0b0b0b","emRelationStroke":"#0b0b0b","attributeBackgroundColorOdd":"#ffffff","attributeBackgroundColorEven":"#f2f2f2","gradientStart":"hsl(210, 28%, 90.3921568627%)","gradientStop":"hsl(210, 25.3846153846%, 30.3921568627%)"}"##;
    const BASE_NESTED: &str = r##"{"background":"#f4f4f4","primaryColor":"#fff4dd","noteBkgColor":"#fff5ad","noteTextColor":"#333","THEME_COLOR_LIMIT":12,"radius":5,"strokeWidth":1,"fontFamily":"\"trebuchet ms\", verdana, arial, sans-serif","fontSize":"16px","useGradient":true,"dropShadow":"drop-shadow( 1px 2px 2px rgba(185,185,185,1))","xyChart":{"backgroundColor":"#f4f4f4","titleColor":"#ff0000","dataLabelColor":"#333","legendTextColor":"#333","xAxisTitleColor":"#333","xAxisLabelColor":"#333","xAxisTickColor":"#333","xAxisLineColor":"#333","yAxisTitleColor":"#333","yAxisLabelColor":"#333","yAxisTickColor":"#333","yAxisLineColor":"#333","plotColorPalette":"#FFF4DD,#FFD8B1,#FFA07A,#ECEFF1,#D6DBDF,#C3E0A8,#FFB6A4,#FFD74D,#738FA7,#FFFFF0"},"primaryTextColor":"#333","secondaryColor":"hsl(-79.4117647059, 100%, 93.3333333333%)","tertiaryColor":"hsl(220.5882352941, 100%, 98.3333333333%)","primaryBorderColor":"hsl(40.5882352941, 60%, 83.3333333333%)","secondaryBorderColor":"hsl(-79.4117647059, 60%, 83.3333333333%)","tertiaryBorderColor":"hsl(220.5882352941, 60%, 88.3333333333%)","noteBorderColor":"hsl(52.6829268293, 60%, 73.9215686275%)","secondaryTextColor":"rgb(11.0000000001, 34.0000000002, 0)","tertiaryTextColor":"rgb(8.5000000002, 5.7500000001, 0)","lineColor":"#0b0b0b","arrowheadColor":"#0b0b0b","textColor":"#333","border2":"hsl(220.5882352941, 60%, 88.3333333333%)","nodeBkg":"#fff4dd","mainBkg":"#fff4dd","nodeBorder":"hsl(40.5882352941, 60%, 83.3333333333%)","clusterBkg":"hsl(220.5882352941, 100%, 98.3333333333%)","clusterBorder":"hsl(220.5882352941, 60%, 88.3333333333%)","defaultLinkColor":"#0b0b0b","titleColor":"rgb(8.5000000002, 5.7500000001, 0)","edgeLabelBackground":"hsl(-79.4117647059, 100%, 93.3333333333%)","flowContainerStroke":"hsl(-79.4117647059, 60%, 83.3333333333%)","nodeTextColor":"#333","actorBorder":"hsl(40.5882352941, 60%, 83.3333333333%)","actorBkg":"#fff4dd","actorTextColor":"#333","actorLineColor":"hsl(40.5882352941, 60%, 83.3333333333%)","labelBoxBkgColor":"#fff4dd","signalColor":"#333","signalTextColor":"#333","labelBoxBorderColor":"hsl(40.5882352941, 60%, 83.3333333333%)","labelTextColor":"#333","loopTextColor":"#333","activationBorderColor":"hsl(-79.4117647059, 100%, 83.3333333333%)","activationBkgColor":"hsl(-79.4117647059, 100%, 93.3333333333%)","sequenceNumberColor":"#f4f4f4","rectBkgColor":"hsl(220.5882352941, 100%, 98.3333333333%)","sectionBkgColor":"hsl(220.5882352941, 100%, 98.3333333333%)","altSectionBkgColor":"white","sectionBkgColor2":"#fff4dd","excludeBkgColor":"#eeeeee","taskBorderColor":"hsl(40.5882352941, 60%, 83.3333333333%)","taskBkgColor":"#fff4dd","activeTaskBorderColor":"#fff4dd","activeTaskBkgColor":"hsl(40.5882352941, 100%, 100%)","gridColor":"lightgrey","doneTaskBkgColor":"lightgrey","doneTaskBorderColor":"grey","critBorderColor":"#ff8888","critBkgColor":"red","todayLineColor":"red","vertLineColor":"navy","taskTextColor":"#333","taskTextOutsideColor":"#333","taskTextLightColor":"#333","taskTextDarkColor":"#333","taskTextClickableColor":"#003163","noteFontWeight":"normal","fontWeight":"normal","personBorder":"hsl(40.5882352941, 60%, 83.3333333333%)","personBkg":"#fff4dd","rowOdd":"hsl(40.5882352941, 100%, 100%)","rowEven":"hsl(40.5882352941, 100%, 98.3333333333%)","transitionColor":"#0b0b0b","transitionLabelColor":"#333","stateLabelColor":"#333","stateBkg":"#fff4dd","labelBackgroundColor":"#fff4dd","compositeBackground":"#f4f4f4","altBackground":"hsl(220.5882352941, 100%, 98.3333333333%)","compositeTitleBackground":"#fff4dd","compositeBorder":"hsl(40.5882352941, 60%, 83.3333333333%)","innerEndBackground":"hsl(40.5882352941, 60%, 83.3333333333%)","errorBkgColor":"hsl(220.5882352941, 100%, 98.3333333333%)","errorTextColor":"rgb(8.5000000002, 5.7500000001, 0)","specialStateColor":"#0b0b0b","cScale0":"hsl(40.5882352941, 100%, 68.3333333333%)","cScale1":"hsl(-79.4117647059, 100%, 68.3333333333%)","cScale2":"hsl(220.5882352941, 100%, 73.3333333333%)","cScale3":"hsl(70.5882352941, 100%, 68.3333333333%)","cScale4":"hsl(100.5882352941, 100%, 68.3333333333%)","cScale5":"hsl(130.5882352941, 100%, 68.3333333333%)","cScale6":"hsl(160.5882352941, 100%, 68.3333333333%)","cScale7":"hsl(190.5882352941, 100%, 68.3333333333%)","cScale8":"hsl(250.5882352941, 100%, 75%)","cScale9":"hsl(310.5882352941, 100%, 68.3333333333%)","cScale10":"hsl(340.5882352941, 100%, 68.3333333333%)","cScale11":"hsl(10.5882352941, 100%, 68.3333333333%)","cScaleInv0":"rgb(0, 52.2500000001, 161.5000000002)","cScaleInv1":"rgb(52.2500000001, 161.5000000002, 0)","cScaleInv2":"rgb(136.0000000002, 92.0000000001, 0)","cScaleInv3":"rgb(28.5, 0, 161.5000000002)","cScaleInv4":"rgb(109.2500000001, 0, 161.5000000002)","cScaleInv5":"rgb(161.5000000002, 0, 133.0000000002)","cScaleInv6":"rgb(161.5000000002, 0, 52.2500000001)","cScaleInv7":"rgb(161.5000000002, 28.5, 0)","cScaleInv8":"rgb(105, 127.5, 0)","cScaleInv9":"rgb(0, 161.5000000002, 28.5)","cScaleInv10":"rgb(0, 161.5000000002, 109.2500000001)","cScaleInv11":"rgb(0, 133.0000000002, 161.5000000002)","cScalePeer0":"hsl(40.5882352941, 100%, 58.3333333333%)","cScalePeer1":"hsl(-79.4117647059, 100%, 58.3333333333%)","cScalePeer2":"hsl(220.5882352941, 100%, 63.3333333333%)","cScalePeer3":"hsl(70.5882352941, 100%, 58.3333333333%)","cScalePeer4":"hsl(100.5882352941, 100%, 58.3333333333%)","cScalePeer5":"hsl(130.5882352941, 100%, 58.3333333333%)","cScalePeer6":"hsl(160.5882352941, 100%, 58.3333333333%)","cScalePeer7":"hsl(190.5882352941, 100%, 58.3333333333%)","cScalePeer8":"hsl(250.5882352941, 100%, 65%)","cScalePeer9":"hsl(310.5882352941, 100%, 58.3333333333%)","cScalePeer10":"hsl(340.5882352941, 100%, 58.3333333333%)","cScalePeer11":"hsl(10.5882352941, 100%, 58.3333333333%)","scaleLabelColor":"#333","cScaleLabel0":"#333","cScaleLabel1":"#333","cScaleLabel2":"#333","cScaleLabel3":"#333","cScaleLabel4":"#333","cScaleLabel5":"#333","cScaleLabel6":"#333","cScaleLabel7":"#333","cScaleLabel8":"#333","cScaleLabel9":"#333","cScaleLabel10":"#333","cScaleLabel11":"#333","surface0":"hsl(220.5882352941, 85%, 88.3333333333%)","surfacePeer0":"hsl(220.5882352941, 85%, 85.3333333333%)","surface1":"hsl(220.5882352941, 85%, 85.3333333333%)","surfacePeer1":"hsl(220.5882352941, 85%, 82.3333333333%)","surface2":"hsl(220.5882352941, 85%, 82.3333333333%)","surfacePeer2":"hsl(220.5882352941, 85%, 79.3333333333%)","surface3":"hsl(220.5882352941, 85%, 79.3333333333%)","surfacePeer3":"hsl(220.5882352941, 85%, 76.3333333333%)","surface4":"hsl(220.5882352941, 85%, 76.3333333333%)","surfacePeer4":"hsl(220.5882352941, 85%, 73.3333333333%)","classText":"#333","fillType0":"#fff4dd","fillType1":"hsl(-79.4117647059, 100%, 93.3333333333%)","fillType2":"hsl(104.5882352941, 100%, 93.3333333333%)","fillType3":"hsl(-15.4117647059, 100%, 93.3333333333%)","fillType4":"hsl(-23.4117647059, 100%, 93.3333333333%)","fillType5":"hsl(-143.4117647059, 100%, 93.3333333333%)","fillType6":"hsl(168.5882352941, 100%, 93.3333333333%)","fillType7":"hsl(48.5882352941, 100%, 93.3333333333%)","pie1":"#fff4dd","pie2":"hsl(-79.4117647059, 100%, 93.3333333333%)","pie3":"hsl(220.5882352941, 100%, 98.3333333333%)","pie4":"hsl(40.5882352941, 100%, 83.3333333333%)","pie5":"hsl(-79.4117647059, 100%, 83.3333333333%)","pie6":"hsl(220.5882352941, 100%, 88.3333333333%)","pie7":"hsl(100.5882352941, 100%, 83.3333333333%)","pie8":"hsl(-19.4117647059, 100%, 83.3333333333%)","pie9":"hsl(160.5882352941, 100%, 93.3333333333%)","pie10":"hsl(100.5882352941, 100%, 73.3333333333%)","pie11":"hsl(-19.4117647059, 100%, 73.3333333333%)","pie12":"hsl(160.5882352941, 100%, 83.3333333333%)","pieTitleTextSize":"25px","pieTitleTextColor":"#333","pieSectionTextSize":"17px","pieSectionTextColor":"#333","pieLegendTextSize":"17px","pieLegendTextColor":"#333","pieStrokeColor":"black","pieStrokeWidth":"2px","pieOuterStrokeWidth":"2px","pieOuterStrokeColor":"black","pieOpacity":"0.7","venn1":"hsl(40.5882352941, 100%, 63.3333333333%)","venn2":"hsl(-79.4117647059, 100%, 63.3333333333%)","venn3":"hsl(220.5882352941, 100%, 68.3333333333%)","venn4":"hsl(100.5882352941, 100%, 63.3333333333%)","venn5":"hsl(-19.4117647059, 100%, 63.3333333333%)","venn6":"hsl(-19.4117647059, 100%, 63.3333333333%)","venn7":"hsl(160.5882352941, 100%, 63.3333333333%)","venn8":"hsl(40.5882352941, 100%, 63.3333333333%)","vennTitleTextColor":"rgb(8.5000000002, 5.7500000001, 0)","vennSetTextColor":"#333","cynefin":{"domainFontSize":16,"itemFontSize":12,"boundaryColor":"#0b0b0b","boundaryWidth":2,"cliffColor":"#8B0000","cliffWidth":4,"arrowColor":"#0b0b0b","arrowWidth":2,"complexBg":"#E8F5E9","complicatedBg":"#E3F2FD","chaoticBg":"#FBE9E7","clearBg":"#FFF8E1","confusionBg":"#F3E5F5","textColor":"#333","labelColor":"#333"},"radar":{"axisColor":"#0b0b0b","axisStrokeWidth":2,"axisLabelFontSize":12,"curveOpacity":0.5,"curveStrokeWidth":2,"graticuleColor":"#DEDEDE","graticuleStrokeWidth":1,"graticuleOpacity":0.3,"legendBoxSize":12,"legendFontSize":12},"wardleyEvolutionColor":"#dc3545","wardley":{"backgroundColor":"#f4f4f4","axisColor":"#0b0b0b","axisTextColor":"#333","gridColor":"lightgrey","componentFill":"#f4f4f4","componentStroke":"#0b0b0b","componentLabelColor":"#333","linkStroke":"#0b0b0b","evolutionStroke":"#dc3545","annotationStroke":"#0b0b0b","annotationTextColor":"#333","annotationFill":"#f4f4f4"},"archEdgeColor":"#777","archEdgeArrowColor":"#777","archEdgeWidth":"3","archGroupBorderColor":"#000","archGroupBorderWidth":"2px","quadrant1Fill":"#fff4dd","quadrant2Fill":"#fff9e2","quadrant3Fill":"#fffee7","quadrant4Fill":"#ffffec","quadrant1TextFill":"#333","quadrant2TextFill":"#2e2e2e","quadrant3TextFill":"#292929","quadrant4TextFill":"#242424","quadrantPointFill":"hsl(40.5882352941, 100%, NaN%)","quadrantPointTextFill":"#333","quadrantXAxisTextFill":"#333","quadrantYAxisTextFill":"#333","quadrantInternalBorderStrokeFill":"hsl(40.5882352941, 60%, 83.3333333333%)","quadrantExternalBorderStrokeFill":"hsl(40.5882352941, 60%, 83.3333333333%)","quadrantTitleFill":"#333","requirementBackground":"#fff4dd","requirementBorderColor":"hsl(40.5882352941, 60%, 83.3333333333%)","requirementBorderSize":"1","requirementTextColor":"#333","relationColor":"#0b0b0b","relationLabelBackground":"hsl(-79.4117647059, 100%, 93.3333333333%)","relationLabelColor":"#333","git0":"hsl(40.5882352941, 100%, 68.3333333333%)","git1":"hsl(-79.4117647059, 100%, 68.3333333333%)","git2":"hsl(220.5882352941, 100%, 73.3333333333%)","git3":"hsl(10.5882352941, 100%, 68.3333333333%)","git4":"hsl(-19.4117647059, 100%, 68.3333333333%)","git5":"hsl(-49.4117647059, 100%, 68.3333333333%)","git6":"hsl(100.5882352941, 100%, 68.3333333333%)","git7":"hsl(160.5882352941, 100%, 68.3333333333%)","gitInv0":"rgb(0, 52.2500000001, 161.5000000002)","gitInv1":"rgb(52.2500000001, 161.5000000002, 0)","gitInv2":"rgb(136.0000000002, 92.0000000001, 0)","gitInv3":"rgb(0, 133.0000000002, 161.5000000002)","gitInv4":"rgb(0, 161.5000000002, 109.2500000001)","gitInv5":"rgb(0, 161.5000000002, 28.5)","gitInv6":"rgb(109.2500000001, 0, 161.5000000002)","gitInv7":"rgb(161.5000000002, 0, 52.2500000001)","branchLabelColor":"#333","gitBranchLabel0":"#333","gitBranchLabel1":"#333","gitBranchLabel2":"#333","gitBranchLabel3":"#333","gitBranchLabel4":"#333","gitBranchLabel5":"#333","gitBranchLabel6":"#333","gitBranchLabel7":"#333","tagLabelColor":"#333","tagLabelBackground":"#fff4dd","tagLabelBorder":"hsl(40.5882352941, 60%, 83.3333333333%)","tagLabelFontSize":"10px","commitLabelColor":"rgb(11.0000000001, 34.0000000002, 0)","commitLabelBackground":"hsl(-79.4117647059, 100%, 93.3333333333%)","commitLabelFontSize":"10px","emUiFill":"white","emUiStroke":"#dbdada","emProcessorFill":"#edb3f6","emProcessorStroke":"#b88cbf","emReadModelFill":"#d3f1a2","emReadModelStroke":"#a3b732","emCommandFill":"#bcd6fe","emCommandStroke":"#679ac3","emEventFill":"#ffb778","emEventStroke":"#c19a0f","emSwimlaneBackgroundOdd":"rgb(250,250,250)","emSwimlaneBackgroundStroke":"rgb(240,240,240)","emArrowhead":"#0b0b0b","emRelationStroke":"#0b0b0b","attributeBackgroundColorOdd":"#ffffff","attributeBackgroundColorEven":"#f2f2f2","gradientStart":"hsl(40.5882352941, 60%, 83.3333333333%)","gradientStop":"hsl(-79.4117647059, 60%, 83.3333333333%)"}"##;
    const BASE_BORDER: &str = r##"{"background":"#f4f4f4","primaryColor":"#fff4dd","noteBkgColor":"#fff5ad","noteTextColor":"#333","THEME_COLOR_LIMIT":12,"radius":5,"strokeWidth":1,"fontFamily":"Arial","fontSize":"16px","useGradient":false,"dropShadow":"drop-shadow( 1px 2px 2px rgba(185,185,185,1))","nodeBorder":"#225577","cScale0":"#123456","primaryTextColor":"#333","secondaryColor":"hsl(-79.4117647059, 100%, 93.3333333333%)","tertiaryColor":"hsl(220.5882352941, 100%, 98.3333333333%)","primaryBorderColor":"hsl(40.5882352941, 60%, 83.3333333333%)","secondaryBorderColor":"hsl(-79.4117647059, 60%, 83.3333333333%)","tertiaryBorderColor":"hsl(220.5882352941, 60%, 88.3333333333%)","noteBorderColor":"hsl(52.6829268293, 60%, 73.9215686275%)","secondaryTextColor":"rgb(11.0000000001, 34.0000000002, 0)","tertiaryTextColor":"rgb(8.5000000002, 5.7500000001, 0)","lineColor":"#0b0b0b","arrowheadColor":"#0b0b0b","textColor":"#333","border2":"hsl(220.5882352941, 60%, 88.3333333333%)","nodeBkg":"#fff4dd","mainBkg":"#fff4dd","clusterBkg":"hsl(220.5882352941, 100%, 98.3333333333%)","clusterBorder":"hsl(220.5882352941, 60%, 88.3333333333%)","defaultLinkColor":"#0b0b0b","titleColor":"rgb(8.5000000002, 5.7500000001, 0)","edgeLabelBackground":"hsl(-79.4117647059, 100%, 93.3333333333%)","flowContainerStroke":"hsl(-79.4117647059, 60%, 83.3333333333%)","nodeTextColor":"#333","actorBorder":"hsl(40.5882352941, 60%, 83.3333333333%)","actorBkg":"#fff4dd","actorTextColor":"#333","actorLineColor":"hsl(40.5882352941, 60%, 83.3333333333%)","labelBoxBkgColor":"#fff4dd","signalColor":"#333","signalTextColor":"#333","labelBoxBorderColor":"hsl(40.5882352941, 60%, 83.3333333333%)","labelTextColor":"#333","loopTextColor":"#333","activationBorderColor":"hsl(-79.4117647059, 100%, 83.3333333333%)","activationBkgColor":"hsl(-79.4117647059, 100%, 93.3333333333%)","sequenceNumberColor":"#f4f4f4","rectBkgColor":"hsl(220.5882352941, 100%, 98.3333333333%)","sectionBkgColor":"hsl(220.5882352941, 100%, 98.3333333333%)","altSectionBkgColor":"white","sectionBkgColor2":"#fff4dd","excludeBkgColor":"#eeeeee","taskBorderColor":"hsl(40.5882352941, 60%, 83.3333333333%)","taskBkgColor":"#fff4dd","activeTaskBorderColor":"#fff4dd","activeTaskBkgColor":"hsl(40.5882352941, 100%, 100%)","gridColor":"lightgrey","doneTaskBkgColor":"lightgrey","doneTaskBorderColor":"grey","critBorderColor":"#ff8888","critBkgColor":"red","todayLineColor":"red","vertLineColor":"navy","taskTextColor":"#333","taskTextOutsideColor":"#333","taskTextLightColor":"#333","taskTextDarkColor":"#333","taskTextClickableColor":"#003163","noteFontWeight":"normal","fontWeight":"normal","personBorder":"hsl(40.5882352941, 60%, 83.3333333333%)","personBkg":"#fff4dd","rowOdd":"hsl(40.5882352941, 100%, 100%)","rowEven":"hsl(40.5882352941, 100%, 98.3333333333%)","transitionColor":"#0b0b0b","transitionLabelColor":"#333","stateLabelColor":"#333","stateBkg":"#fff4dd","labelBackgroundColor":"#fff4dd","compositeBackground":"#f4f4f4","altBackground":"hsl(220.5882352941, 100%, 98.3333333333%)","compositeTitleBackground":"#fff4dd","compositeBorder":"#225577","innerEndBackground":"#225577","errorBkgColor":"hsl(220.5882352941, 100%, 98.3333333333%)","errorTextColor":"rgb(8.5000000002, 5.7500000001, 0)","specialStateColor":"#0b0b0b","cScale1":"hsl(-79.4117647059, 100%, 68.3333333333%)","cScale2":"hsl(220.5882352941, 100%, 73.3333333333%)","cScale3":"hsl(70.5882352941, 100%, 68.3333333333%)","cScale4":"hsl(100.5882352941, 100%, 68.3333333333%)","cScale5":"hsl(130.5882352941, 100%, 68.3333333333%)","cScale6":"hsl(160.5882352941, 100%, 68.3333333333%)","cScale7":"hsl(190.5882352941, 100%, 68.3333333333%)","cScale8":"hsl(250.5882352941, 100%, 75%)","cScale9":"hsl(310.5882352941, 100%, 68.3333333333%)","cScale10":"hsl(340.5882352941, 100%, 68.3333333333%)","cScale11":"hsl(10.5882352941, 100%, 68.3333333333%)","cScaleInv0":"#ffffff","cScaleInv1":"rgb(52.2500000001, 161.5000000002, 0)","cScaleInv2":"rgb(136.0000000002, 92.0000000001, 0)","cScaleInv3":"rgb(28.5, 0, 161.5000000002)","cScaleInv4":"rgb(109.2500000001, 0, 161.5000000002)","cScaleInv5":"rgb(161.5000000002, 0, 133.0000000002)","cScaleInv6":"rgb(161.5000000002, 0, 52.2500000001)","cScaleInv7":"rgb(161.5000000002, 28.5, 0)","cScaleInv8":"rgb(105, 127.5, 0)","cScaleInv9":"rgb(0, 161.5000000002, 28.5)","cScaleInv10":"rgb(0, 161.5000000002, 109.2500000001)","cScaleInv11":"rgb(0, 133.0000000002, 161.5000000002)","cScalePeer0":"hsl(210, 65.3846153846%, 0%)","cScalePeer1":"hsl(-79.4117647059, 100%, 58.3333333333%)","cScalePeer2":"hsl(220.5882352941, 100%, 63.3333333333%)","cScalePeer3":"hsl(70.5882352941, 100%, 58.3333333333%)","cScalePeer4":"hsl(100.5882352941, 100%, 58.3333333333%)","cScalePeer5":"hsl(130.5882352941, 100%, 58.3333333333%)","cScalePeer6":"hsl(160.5882352941, 100%, 58.3333333333%)","cScalePeer7":"hsl(190.5882352941, 100%, 58.3333333333%)","cScalePeer8":"hsl(250.5882352941, 100%, 65%)","cScalePeer9":"hsl(310.5882352941, 100%, 58.3333333333%)","cScalePeer10":"hsl(340.5882352941, 100%, 58.3333333333%)","cScalePeer11":"hsl(10.5882352941, 100%, 58.3333333333%)","scaleLabelColor":"#333","cScaleLabel0":"#333","cScaleLabel1":"#333","cScaleLabel2":"#333","cScaleLabel3":"#333","cScaleLabel4":"#333","cScaleLabel5":"#333","cScaleLabel6":"#333","cScaleLabel7":"#333","cScaleLabel8":"#333","cScaleLabel9":"#333","cScaleLabel10":"#333","cScaleLabel11":"#333","surface0":"hsl(220.5882352941, 85%, 88.3333333333%)","surfacePeer0":"hsl(220.5882352941, 85%, 85.3333333333%)","surface1":"hsl(220.5882352941, 85%, 85.3333333333%)","surfacePeer1":"hsl(220.5882352941, 85%, 82.3333333333%)","surface2":"hsl(220.5882352941, 85%, 82.3333333333%)","surfacePeer2":"hsl(220.5882352941, 85%, 79.3333333333%)","surface3":"hsl(220.5882352941, 85%, 79.3333333333%)","surfacePeer3":"hsl(220.5882352941, 85%, 76.3333333333%)","surface4":"hsl(220.5882352941, 85%, 76.3333333333%)","surfacePeer4":"hsl(220.5882352941, 85%, 73.3333333333%)","classText":"#333","fillType0":"#fff4dd","fillType1":"hsl(-79.4117647059, 100%, 93.3333333333%)","fillType2":"hsl(104.5882352941, 100%, 93.3333333333%)","fillType3":"hsl(-15.4117647059, 100%, 93.3333333333%)","fillType4":"hsl(-23.4117647059, 100%, 93.3333333333%)","fillType5":"hsl(-143.4117647059, 100%, 93.3333333333%)","fillType6":"hsl(168.5882352941, 100%, 93.3333333333%)","fillType7":"hsl(48.5882352941, 100%, 93.3333333333%)","pie1":"#fff4dd","pie2":"hsl(-79.4117647059, 100%, 93.3333333333%)","pie3":"hsl(220.5882352941, 100%, 98.3333333333%)","pie4":"hsl(40.5882352941, 100%, 83.3333333333%)","pie5":"hsl(-79.4117647059, 100%, 83.3333333333%)","pie6":"hsl(220.5882352941, 100%, 88.3333333333%)","pie7":"hsl(100.5882352941, 100%, 83.3333333333%)","pie8":"hsl(-19.4117647059, 100%, 83.3333333333%)","pie9":"hsl(160.5882352941, 100%, 93.3333333333%)","pie10":"hsl(100.5882352941, 100%, 73.3333333333%)","pie11":"hsl(-19.4117647059, 100%, 73.3333333333%)","pie12":"hsl(160.5882352941, 100%, 83.3333333333%)","pieTitleTextSize":"25px","pieTitleTextColor":"#333","pieSectionTextSize":"17px","pieSectionTextColor":"#333","pieLegendTextSize":"17px","pieLegendTextColor":"#333","pieStrokeColor":"black","pieStrokeWidth":"2px","pieOuterStrokeWidth":"2px","pieOuterStrokeColor":"black","pieOpacity":"0.7","venn1":"hsl(40.5882352941, 100%, 63.3333333333%)","venn2":"hsl(-79.4117647059, 100%, 63.3333333333%)","venn3":"hsl(220.5882352941, 100%, 68.3333333333%)","venn4":"hsl(100.5882352941, 100%, 63.3333333333%)","venn5":"hsl(-19.4117647059, 100%, 63.3333333333%)","venn6":"hsl(-19.4117647059, 100%, 63.3333333333%)","venn7":"hsl(160.5882352941, 100%, 63.3333333333%)","venn8":"hsl(40.5882352941, 100%, 63.3333333333%)","vennTitleTextColor":"rgb(8.5000000002, 5.7500000001, 0)","vennSetTextColor":"#333","cynefin":{"domainFontSize":16,"itemFontSize":12,"boundaryColor":"#0b0b0b","boundaryWidth":2,"cliffColor":"#8B0000","cliffWidth":4,"arrowColor":"#0b0b0b","arrowWidth":2,"complexBg":"#E8F5E9","complicatedBg":"#E3F2FD","chaoticBg":"#FBE9E7","clearBg":"#FFF8E1","confusionBg":"#F3E5F5","textColor":"#333","labelColor":"#333"},"radar":{"axisColor":"#0b0b0b","axisStrokeWidth":2,"axisLabelFontSize":12,"curveOpacity":0.5,"curveStrokeWidth":2,"graticuleColor":"#DEDEDE","graticuleStrokeWidth":1,"graticuleOpacity":0.3,"legendBoxSize":12,"legendFontSize":12},"wardleyEvolutionColor":"#dc3545","wardley":{"backgroundColor":"#f4f4f4","axisColor":"#0b0b0b","axisTextColor":"#333","gridColor":"lightgrey","componentFill":"#f4f4f4","componentStroke":"#0b0b0b","componentLabelColor":"#333","linkStroke":"#0b0b0b","evolutionStroke":"#dc3545","annotationStroke":"#0b0b0b","annotationTextColor":"#333","annotationFill":"#f4f4f4"},"archEdgeColor":"#777","archEdgeArrowColor":"#777","archEdgeWidth":"3","archGroupBorderColor":"#000","archGroupBorderWidth":"2px","quadrant1Fill":"#fff4dd","quadrant2Fill":"#fff9e2","quadrant3Fill":"#fffee7","quadrant4Fill":"#ffffec","quadrant1TextFill":"#333","quadrant2TextFill":"#2e2e2e","quadrant3TextFill":"#292929","quadrant4TextFill":"#242424","quadrantPointFill":"hsl(40.5882352941, 100%, NaN%)","quadrantPointTextFill":"#333","quadrantXAxisTextFill":"#333","quadrantYAxisTextFill":"#333","quadrantInternalBorderStrokeFill":"hsl(40.5882352941, 60%, 83.3333333333%)","quadrantExternalBorderStrokeFill":"hsl(40.5882352941, 60%, 83.3333333333%)","quadrantTitleFill":"#333","xyChart":{"backgroundColor":"#f4f4f4","titleColor":"#333","dataLabelColor":"#333","legendTextColor":"#333","xAxisTitleColor":"#333","xAxisLabelColor":"#333","xAxisTickColor":"#333","xAxisLineColor":"#333","yAxisTitleColor":"#333","yAxisLabelColor":"#333","yAxisTickColor":"#333","yAxisLineColor":"#333","plotColorPalette":"#FFF4DD,#FFD8B1,#FFA07A,#ECEFF1,#D6DBDF,#C3E0A8,#FFB6A4,#FFD74D,#738FA7,#FFFFF0"},"requirementBackground":"#fff4dd","requirementBorderColor":"hsl(40.5882352941, 60%, 83.3333333333%)","requirementBorderSize":"1","requirementTextColor":"#333","relationColor":"#0b0b0b","relationLabelBackground":"hsl(-79.4117647059, 100%, 93.3333333333%)","relationLabelColor":"#333","git0":"hsl(40.5882352941, 100%, 68.3333333333%)","git1":"hsl(-79.4117647059, 100%, 68.3333333333%)","git2":"hsl(220.5882352941, 100%, 73.3333333333%)","git3":"hsl(10.5882352941, 100%, 68.3333333333%)","git4":"hsl(-19.4117647059, 100%, 68.3333333333%)","git5":"hsl(-49.4117647059, 100%, 68.3333333333%)","git6":"hsl(100.5882352941, 100%, 68.3333333333%)","git7":"hsl(160.5882352941, 100%, 68.3333333333%)","gitInv0":"rgb(0, 52.2500000001, 161.5000000002)","gitInv1":"rgb(52.2500000001, 161.5000000002, 0)","gitInv2":"rgb(136.0000000002, 92.0000000001, 0)","gitInv3":"rgb(0, 133.0000000002, 161.5000000002)","gitInv4":"rgb(0, 161.5000000002, 109.2500000001)","gitInv5":"rgb(0, 161.5000000002, 28.5)","gitInv6":"rgb(109.2500000001, 0, 161.5000000002)","gitInv7":"rgb(161.5000000002, 0, 52.2500000001)","branchLabelColor":"#333","gitBranchLabel0":"#333","gitBranchLabel1":"#333","gitBranchLabel2":"#333","gitBranchLabel3":"#333","gitBranchLabel4":"#333","gitBranchLabel5":"#333","gitBranchLabel6":"#333","gitBranchLabel7":"#333","tagLabelColor":"#333","tagLabelBackground":"#fff4dd","tagLabelBorder":"hsl(40.5882352941, 60%, 83.3333333333%)","tagLabelFontSize":"10px","commitLabelColor":"rgb(11.0000000001, 34.0000000002, 0)","commitLabelBackground":"hsl(-79.4117647059, 100%, 93.3333333333%)","commitLabelFontSize":"10px","emUiFill":"white","emUiStroke":"#dbdada","emProcessorFill":"#edb3f6","emProcessorStroke":"#b88cbf","emReadModelFill":"#d3f1a2","emReadModelStroke":"#a3b732","emCommandFill":"#bcd6fe","emCommandStroke":"#679ac3","emEventFill":"#ffb778","emEventStroke":"#c19a0f","emSwimlaneBackgroundOdd":"rgb(250,250,250)","emSwimlaneBackgroundStroke":"rgb(240,240,240)","emArrowhead":"#0b0b0b","emRelationStroke":"#0b0b0b","attributeBackgroundColorOdd":"#ffffff","attributeBackgroundColorEven":"#f2f2f2","gradientStart":"hsl(40.5882352941, 60%, 83.3333333333%)","gradientStop":"hsl(-79.4117647059, 60%, 83.3333333333%)"}"##;

    fn overrides(name: &str) -> Map<String, Value> {
        let value = match name {
            "none" => json!({}),
            "colors" => {
                json!({ "primaryColor": "#abcdef", "secondaryColor": "#123456", "darkMode": true })
            }
            "nested" => json!({ "xyChart": { "titleColor": "#ff0000" } }),
            "border" => {
                json!({ "nodeBorder": "#225577", "cScale0": "#123456", "fontFamily": "Arial" })
            }
            other => panic!("unknown override set {other}"),
        };
        value.as_object().expect("object").clone()
    }

    fn fixtures() -> Vec<(&'static str, &'static str, &'static str)> {
        vec![
            ("default", "none", DEFAULT_NONE),
            ("default", "colors", DEFAULT_COLORS),
            ("default", "nested", DEFAULT_NESTED),
            ("default", "border", DEFAULT_BORDER),
            ("dark", "none", DARK_NONE),
            ("dark", "colors", DARK_COLORS),
            ("dark", "nested", DARK_NESTED),
            ("dark", "border", DARK_BORDER),
            ("forest", "none", FOREST_NONE),
            ("forest", "colors", FOREST_COLORS),
            ("forest", "nested", FOREST_NESTED),
            ("forest", "border", FOREST_BORDER),
            ("neutral", "none", NEUTRAL_NONE),
            ("neutral", "colors", NEUTRAL_COLORS),
            ("neutral", "nested", NEUTRAL_NESTED),
            ("neutral", "border", NEUTRAL_BORDER),
            ("base", "none", BASE_NONE),
            ("base", "colors", BASE_COLORS),
            ("base", "nested", BASE_NESTED),
            ("base", "border", BASE_BORDER),
        ]
    }

    /// Every variable of every theme, with no overrides and with a few, compared with what
    /// upstream produced on node. The fixtures are the JSON getThemeVariables returned.
    #[test]
    fn matches_upstream_theme_variables() {
        let mut failures = Vec::new();
        for (theme, set, expected) in fixtures() {
            let expected: Value = serde_json::from_str(expected).expect("fixture json");
            let actual = Theme::from_name(theme)
                .expect("implemented theme")
                .variables(&overrides(set))
                .unwrap_or_else(|error| panic!("{theme}/{set}: {error}"))
                .to_json();
            if actual != expected {
                let mut differences = Vec::new();
                if let (Value::Object(actual), Value::Object(expected)) = (&actual, &expected) {
                    for (key, value) in expected {
                        match actual.get(key) {
                            Some(actual_value) if actual_value == value => {}
                            Some(actual_value) => differences
                                .push(format!("{key}: expected {value}, got {actual_value}")),
                            None => differences.push(format!("{key}: expected {value}, missing")),
                        }
                    }
                    for key in actual.keys() {
                        if !expected.contains_key(key) {
                            differences.push(format!("{key}: unexpected"));
                        }
                    }
                }
                differences.sort();
                differences.truncate(12);
                failures.push(format!("{theme}/{set}: {}", differences.join("; ")));
            }
        }
        assert!(failures.is_empty(), "{failures:#?}");
    }
}
#[cfg(test)]
mod color_tests {
    use super::color;
    use pretty_assertions::assert_eq;

    type AdjustCase<'a> = (&'a str, &'a [(&'a str, f64)], &'a str);

    #[test]
    fn adjust_moves_rgb_channels() {
        let cases: &[AdjustCase<'_>] = &[
            ("rgb(0, 0, 0)", &[("r", 100.0)], "#640000"),
            ("rgb(0, 0, 0)", &[("g", 100.0)], "#006400"),
            ("rgb(0, 0, 0)", &[("b", 100.0)], "#000064"),
            ("rgb(0, 0, 0)", &[("r", 100.0), ("g", 100.0)], "#646400"),
            (
                "rgb(0, 0, 0)",
                &[("r", 100.0), ("g", 100.0), ("b", 100.0)],
                "#646464",
            ),
            ("rgb(255, 255, 255)", &[("r", -100.0)], "#9bffff"),
            ("rgb(255, 255, 255)", &[("g", -100.0)], "#ff9bff"),
            ("rgb(255, 255, 255)", &[("b", -100.0)], "#ffff9b"),
            ("rgb(200, 0, 0)", &[("r", 100.0)], "#ff0000"),
            ("rgb(100, 0, 0)", &[("r", -200.0)], "#000000"),
        ];
        for (color_value, channels, expected) in cases {
            assert_eq!(
                color::adjust(color_value, channels).expect("valid color"),
                *expected
            );
        }
    }

    #[test]
    fn adjust_moves_hsl_channels() {
        let cases: &[AdjustCase<'_>] = &[
            ("hsl(0, 50%, 50%)", &[("h", 100.0)], "hsl(100, 50%, 50%)"),
            ("hsl(0, 50%, 50%)", &[("s", 25.0)], "hsl(0, 75%, 50%)"),
            ("hsl(0, 50%, 50%)", &[("l", 25.0)], "hsl(0, 50%, 75%)"),
            (
                "hsl(0, 50%, 50%)",
                &[("h", 100.0), ("s", 25.0)],
                "hsl(100, 75%, 50%)",
            ),
            (
                "hsl(0, 50%, 50%)",
                &[("h", 100.0), ("s", 25.0), ("l", 25.0)],
                "hsl(100, 75%, 75%)",
            ),
            ("hsl(100, 50%, 50%)", &[("h", -100.0)], "hsl(0, 50%, 50%)"),
            ("hsl(0, 50%, 50%)", &[("s", -25.0)], "hsl(0, 25%, 50%)"),
            ("hsl(0, 50%, 50%)", &[("l", -25.0)], "hsl(0, 50%, 25%)"),
            ("hsl(300, 50%, 50%)", &[("h", 100.0)], "hsl(40, 50%, 50%)"),
            ("hsl(0, 50%, 50%)", &[("h", -100.0)], "hsl(-100, 50%, 50%)"),
            ("hsl(0, 100%, 50%)", &[("s", 25.0)], "hsl(0, 100%, 50%)"),
            ("hsl(0, 0%, 50%)", &[("s", -25.0)], "hsl(0, 0%, 50%)"),
            (
                "hsla(0, 0%, 50%, .5)",
                &[("s", -25.0)],
                "hsla(0, 0%, 50%, 0.5)",
            ),
        ];
        for (color_value, channels, expected) in cases {
            assert_eq!(
                color::adjust(color_value, channels).expect("valid color"),
                *expected
            );
        }
    }

    #[test]
    fn adjust_moves_the_alpha_channel() {
        let cases: &[AdjustCase<'_>] = &[
            ("rgba(0, 0, 0, 0)", &[("a", 0.5)], "rgba(0, 0, 0, 0.5)"),
            ("hsla(0, 0%, 0%, 0)", &[("a", 0.5)], "hsla(0, 0%, 0%, 0.5)"),
            ("rgba(0, 0, 0, 0)", &[("a", 1.0)], "#000000"),
            ("rgba(0, 0, 0, 1)", &[("a", -0.5)], "rgba(0, 0, 0, 0.5)"),
            ("rgba(0, 0, 0, 1)", &[("a", -1.0)], "rgba(0, 0, 0, 0)"),
            ("rgba(0, 0, 0, 0.5)", &[("a", 1.0)], "#000000"),
            ("rgba(0, 0, 0, 0.5)", &[("a", -1.0)], "rgba(0, 0, 0, 0)"),
        ];
        for (color_value, channels, expected) in cases {
            assert_eq!(
                color::adjust(color_value, channels).expect("valid color"),
                *expected
            );
        }
    }

    #[test]
    fn adjust_refuses_rgb_and_hsl_at_the_same_time() {
        for channels in [
            vec![("r", 10.0), ("h", 10.0)],
            vec![("g", 10.0), ("l", 10.0)],
            vec![("b", 10.0), ("s", 10.0)],
        ] {
            let error = color::adjust("#000", &channels).expect_err("mixed channels");
            assert_eq!(
                error.to_string(),
                "Cannot change both RGB and HSL channels at the same time"
            );
        }
    }

    #[test]
    fn lighten_increases_lightness() {
        let cases = [
            ("hsl(0, 0%, 0%)", 0.0, "hsl(0, 0%, 0%)"),
            ("hsl(0, 0%, 0%)", 50.0, "hsl(0, 0%, 50%)"),
            ("hsl(0, 0%, 0%)", 75.0, "hsl(0, 0%, 75%)"),
            ("hsl(0, 0%, 0%)", 100.0, "hsl(0, 0%, 100%)"),
            ("hsl(0, 0%, 50%)", 100.0, "hsl(0, 0%, 100%)"),
            ("hsl(0, 0%, 100%)", 100.0, "hsl(0, 0%, 100%)"),
        ];
        for (color_value, amount, expected) in cases {
            assert_eq!(
                color::lighten(color_value, amount).expect("valid color"),
                expected
            );
        }
    }

    #[test]
    fn darken_decreases_lightness() {
        let cases = [
            ("hsl(0, 0%, 100%)", 0.0, "hsl(0, 0%, 100%)"),
            ("hsl(0, 0%, 100%)", 50.0, "hsl(0, 0%, 50%)"),
            ("hsl(0, 0%, 100%)", 75.0, "hsl(0, 0%, 25%)"),
            ("hsl(0, 0%, 100%)", 100.0, "hsl(0, 0%, 0%)"),
            ("hsl(0, 0%, 50%)", 100.0, "hsl(0, 0%, 0%)"),
        ];
        for (color_value, amount, expected) in cases {
            assert_eq!(
                color::darken(color_value, amount).expect("valid color"),
                expected
            );
        }
    }

    #[test]
    fn invert_gets_the_inverse() {
        assert_eq!(color::invert("#b37399", 100.0).expect("hex"), "#4c8c66");
        assert_eq!(color::invert("black", 100.0).expect("keyword"), "#ffffff");
        assert_eq!(
            color::invert("#550e0c", 20.0).expect("weight"),
            "rgb(102, 59.4, 58.2)"
        );
    }

    #[test]
    fn is_dark_checks_luminance() {
        for (color_value, expected) in [
            ("#000000", true),
            ("#8a8a8a", true),
            ("#bbbbbb", true),
            ("#ffcc00", false),
            ("#e0e0e0", false),
            ("#ffffff", false),
        ] {
            assert_eq!(color::is_dark(color_value).expect("valid color"), expected);
        }
    }

    #[test]
    fn rgba_clamps_and_stringifies() {
        let cases = [
            ((0.0, 0.0, 0.0, 0.0), "rgba(0, 0, 0, 0)"),
            ((255.0, 255.0, 255.0, 0.5), "rgba(255, 255, 255, 0.5)"),
            ((0.0, 0.0, 0.0, 1.0), "#000000"),
            ((128.0, 128.0, 128.0, 1.0), "#808080"),
            ((-1.0, -1.0, -1.0, -1.0), "rgba(0, 0, 0, 0)"),
            ((1000.0, 1000.0, 1000.0, 1000.0), "#ffffff"),
        ];
        for ((r, g, b, a), expected) in cases {
            assert_eq!(color::rgba(r, g, b, a), expected);
        }
    }

    #[test]
    fn parses_every_color_shape_the_themes_and_overrides_use() {
        assert_eq!(
            color::lighten("#abc", 10.0).expect("short hex"),
            "hsl(210, 25%, 83.3333333333%)"
        );
        assert_eq!(
            color::lighten("#aabbcc", 0.0).expect("long hex unchanged"),
            "#aabbcc"
        );
        assert_eq!(
            color::adjust("rgb(10 20 30 / 50%)", &[("a", -0.1)]).expect("rgb"),
            "rgba(10, 20, 30, 0.4)"
        );
        assert_eq!(
            color::adjust("hsl(240 100% 50% / 0.5)", &[("a", -0.1)]).expect("hsl"),
            "hsla(240, 100%, 50%, 0.4)"
        );
        assert_eq!(
            color::adjust("lightgrey", &[("a", -0.1)]).expect("keyword"),
            "rgba(211, 211, 211, 0.9)"
        );
    }

    #[test]
    fn an_unparseable_color_is_an_error() {
        let error = color::lighten("not-a-color", 10.0).expect_err("unknown format");
        assert_eq!(
            error.to_string(),
            "Unsupported color format: \"not-a-color\""
        );
    }
}

#[cfg(test)]
mod theme_specs {
    use super::*;
    use serde_json::json;

    fn variables(theme: Theme, overrides: Value) -> Map<String, Value> {
        theme
            .variables(overrides.as_object().expect("object"))
            .expect("valid theme")
            .into_map()
    }

    #[test]
    fn theme_dark_gantt_and_pie_colors_are_readable_on_the_dark_canvas() {
        let dark = variables(Theme::Dark, json!({}));
        assert_eq!(dark.get("taskTextDarkColor"), Some(&json!("#2c2c2c")));
        assert_eq!(dark.get("pieTitleTextColor"), Some(&json!("lightgrey")));
        assert_eq!(dark.get("pieLegendTextColor"), Some(&json!("lightgrey")));
    }

    #[test]
    fn every_theme_defaults_the_legend_text_to_the_primary_text_color() {
        for theme in Theme::ALL {
            let variables = variables(theme, json!({}));
            assert_eq!(
                variables["xyChart"]["legendTextColor"],
                variables["primaryTextColor"],
                "{}",
                theme.name()
            );
        }
    }

    #[test]
    fn base_keeps_the_gradient_until_node_border_is_overridden() {
        assert_eq!(
            variables(Theme::Base, json!({})).get("useGradient"),
            Some(&json!(true))
        );
        assert_eq!(
            variables(Theme::Base, json!({ "mainBkg": "#ffe1ef" })).get("useGradient"),
            Some(&json!(true))
        );

        let overridden = variables(Theme::Base, json!({ "nodeBorder": "#225577" }));
        assert_eq!(overridden.get("nodeBorder"), Some(&json!("#225577")));
        assert_eq!(overridden.get("useGradient"), Some(&json!(false)));

        let both = variables(
            Theme::Base,
            json!({ "nodeBorder": "#225577", "useGradient": true }),
        );
        assert_eq!(both.get("nodeBorder"), Some(&json!("#225577")));
        assert_eq!(both.get("useGradient"), Some(&json!(true)));

        assert_eq!(
            variables(Theme::Base, json!({ "useGradient": false })).get("useGradient"),
            Some(&json!(false))
        );
    }

    #[test]
    fn object_valued_variables_keep_the_keys_an_override_leaves_out() {
        for theme in Theme::ALL {
            let generated = variables(theme, json!({}));
            for (variable, value) in &generated {
                let Value::Object(object) = value else {
                    continue;
                };
                if object.len() < 2 {
                    continue;
                }
                let first = object.keys().next().expect("a key");
                let mut inner = Map::new();
                inner.insert(first.clone(), json!("#123456"));
                let mut outer = Map::new();
                outer.insert(variable.clone(), Value::Object(inner));
                let overridden = variables(theme, Value::Object(outer));
                let overridden_object = overridden[variable]
                    .as_object()
                    .expect("object-valued override");
                assert_eq!(overridden_object[first], json!("#123456"));
                for (key, expected) in object {
                    if key != first {
                        assert_eq!(
                            overridden_object.get(key),
                            Some(expected),
                            "{}/{}",
                            theme.name(),
                            variable
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn a_scalar_variable_is_replaced() {
        assert_eq!(
            variables(Theme::Dark, json!({ "primaryColor": "#abcdef" })).get("primaryColor"),
            Some(&json!("#abcdef"))
        );
    }

    #[test]
    fn every_theme_defines_and_honours_the_agentflow_container_stroke() {
        for theme in Theme::ALL {
            let generated = variables(theme, json!({}));
            assert!(
                generated.get("flowContainerStroke").is_some_and(truthy),
                "{}",
                theme.name()
            );
            let overridden = variables(theme, json!({ "flowContainerStroke": "#abcdef" }));
            assert_eq!(
                overridden.get("flowContainerStroke"),
                Some(&json!("#abcdef")),
                "{}",
                theme.name()
            );
        }
    }

    #[test]
    fn a_theme_override_with_a_bad_color_reports_it() {
        let error = Theme::Default
            .variables(
                json!({ "primaryColor": "not-a-color" })
                    .as_object()
                    .expect("object"),
            )
            .expect_err("bad color");
        assert!(error.to_string().contains("Unsupported color format"));
    }

    #[test]
    fn the_five_theme_names_round_trip() {
        for theme in Theme::ALL {
            assert_eq!(Theme::from_name(theme.name()), Some(theme));
        }
        assert_eq!(Theme::from_name("neo"), None);
        assert_eq!(Theme::from_name("null"), None);
    }
}
