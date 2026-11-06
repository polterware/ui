//! Electron architecture's `ServiceMark` (`domain/service-marks.tsx`): the same filled
//! SVG paths and `ICON_TILE_BRAND_ACCENTS`. A static mark like gpui-kit's `Icon`;
//! it owns no state, timers, or animation. Unknown platforms fall back to Cloud.
use crate::{
    icons::{Icon, icon_colored},
    theme::Theme,
};
use gpui::{AnyElement, App, IntoElement, SharedString, prelude::*, px, rgb, svg};
use std::borrow::Cow;

/// Canonical mark key; the integrations catalog calls GCP `google-cloud`.
pub fn key(name: &str) -> &str {
    if name == "google-cloud" { "gcp" } else { name }
}

/// Embedded assets copied from Electron's `ServiceMark` paths.
pub(crate) fn asset(path: &str) -> Option<Cow<'static, [u8]>> {
    match path {
        "service-marks/auth0.svg" => Some(Cow::Borrowed(include_bytes!(
            "../assets/service-marks/auth0.svg"
        ))),
        "service-marks/aws.svg" => Some(Cow::Borrowed(include_bytes!(
            "../assets/service-marks/aws.svg"
        ))),
        "service-marks/azure.svg" => Some(Cow::Borrowed(include_bytes!(
            "../assets/service-marks/azure.svg"
        ))),
        "service-marks/bigquery.svg" => Some(Cow::Borrowed(include_bytes!(
            "../assets/service-marks/bigquery.svg"
        ))),
        "service-marks/clerk.svg" => Some(Cow::Borrowed(include_bytes!(
            "../assets/service-marks/clerk.svg"
        ))),
        "service-marks/cloudflare.svg" => Some(Cow::Borrowed(include_bytes!(
            "../assets/service-marks/cloudflare.svg"
        ))),
        "service-marks/datadog.svg" => Some(Cow::Borrowed(include_bytes!(
            "../assets/service-marks/datadog.svg"
        ))),
        "service-marks/digitalocean.svg" => Some(Cow::Borrowed(include_bytes!(
            "../assets/service-marks/digitalocean.svg"
        ))),
        "service-marks/docker.svg" => Some(Cow::Borrowed(include_bytes!(
            "../assets/service-marks/docker.svg"
        ))),
        "service-marks/elasticsearch.svg" => Some(Cow::Borrowed(include_bytes!(
            "../assets/service-marks/elasticsearch.svg"
        ))),
        "service-marks/fly.svg" => Some(Cow::Borrowed(include_bytes!(
            "../assets/service-marks/fly.svg"
        ))),
        "service-marks/gcp.svg" => Some(Cow::Borrowed(include_bytes!(
            "../assets/service-marks/gcp.svg"
        ))),
        "service-marks/github.svg" => Some(Cow::Borrowed(include_bytes!(
            "../assets/service-marks/github.svg"
        ))),
        "service-marks/github_actions.svg" => Some(Cow::Borrowed(include_bytes!(
            "../assets/service-marks/github_actions.svg"
        ))),
        "service-marks/gitlab.svg" => Some(Cow::Borrowed(include_bytes!(
            "../assets/service-marks/gitlab.svg"
        ))),
        "service-marks/grafana.svg" => Some(Cow::Borrowed(include_bytes!(
            "../assets/service-marks/grafana.svg"
        ))),
        "service-marks/graphql.svg" => Some(Cow::Borrowed(include_bytes!(
            "../assets/service-marks/graphql.svg"
        ))),
        "service-marks/kafka.svg" => Some(Cow::Borrowed(include_bytes!(
            "../assets/service-marks/kafka.svg"
        ))),
        "service-marks/kubernetes.svg" => Some(Cow::Borrowed(include_bytes!(
            "../assets/service-marks/kubernetes.svg"
        ))),
        "service-marks/lambda.svg" => Some(Cow::Borrowed(include_bytes!(
            "../assets/service-marks/lambda.svg"
        ))),
        "service-marks/mongodb.svg" => Some(Cow::Borrowed(include_bytes!(
            "../assets/service-marks/mongodb.svg"
        ))),
        "service-marks/mysql.svg" => Some(Cow::Borrowed(include_bytes!(
            "../assets/service-marks/mysql.svg"
        ))),
        "service-marks/netlify.svg" => Some(Cow::Borrowed(include_bytes!(
            "../assets/service-marks/netlify.svg"
        ))),
        "service-marks/nextjs.svg" => Some(Cow::Borrowed(include_bytes!(
            "../assets/service-marks/nextjs.svg"
        ))),
        "service-marks/nginx.svg" => Some(Cow::Borrowed(include_bytes!(
            "../assets/service-marks/nginx.svg"
        ))),
        "service-marks/openai.svg" => Some(Cow::Borrowed(include_bytes!(
            "../assets/service-marks/openai.svg"
        ))),
        "service-marks/postgres.svg" => Some(Cow::Borrowed(include_bytes!(
            "../assets/service-marks/postgres.svg"
        ))),
        "service-marks/prisma.svg" => Some(Cow::Borrowed(include_bytes!(
            "../assets/service-marks/prisma.svg"
        ))),
        "service-marks/prometheus.svg" => Some(Cow::Borrowed(include_bytes!(
            "../assets/service-marks/prometheus.svg"
        ))),
        "service-marks/rabbitmq.svg" => Some(Cow::Borrowed(include_bytes!(
            "../assets/service-marks/rabbitmq.svg"
        ))),
        "service-marks/railway.svg" => Some(Cow::Borrowed(include_bytes!(
            "../assets/service-marks/railway.svg"
        ))),
        "service-marks/redis.svg" => Some(Cow::Borrowed(include_bytes!(
            "../assets/service-marks/redis.svg"
        ))),
        "service-marks/render.svg" => Some(Cow::Borrowed(include_bytes!(
            "../assets/service-marks/render.svg"
        ))),
        "service-marks/s3.svg" => Some(Cow::Borrowed(include_bytes!(
            "../assets/service-marks/s3.svg"
        ))),
        "service-marks/snowflake.svg" => Some(Cow::Borrowed(include_bytes!(
            "../assets/service-marks/snowflake.svg"
        ))),
        "service-marks/sqs.svg" => Some(Cow::Borrowed(include_bytes!(
            "../assets/service-marks/sqs.svg"
        ))),
        "service-marks/stripe.svg" => Some(Cow::Borrowed(include_bytes!(
            "../assets/service-marks/stripe.svg"
        ))),
        "service-marks/supabase.svg" => Some(Cow::Borrowed(include_bytes!(
            "../assets/service-marks/supabase.svg"
        ))),
        "service-marks/terraform.svg" => Some(Cow::Borrowed(include_bytes!(
            "../assets/service-marks/terraform.svg"
        ))),
        "service-marks/vercel.svg" => Some(Cow::Borrowed(include_bytes!(
            "../assets/service-marks/vercel.svg"
        ))),
        _ => None,
    }
}
/// Known marks, for catalogs and galleries.
pub const ALL: &[&str] = &[
    "auth0",
    "aws",
    "azure",
    "bigquery",
    "clerk",
    "cloudflare",
    "datadog",
    "digitalocean",
    "docker",
    "elasticsearch",
    "fly",
    "gcp",
    "github",
    "github_actions",
    "gitlab",
    "grafana",
    "graphql",
    "kafka",
    "kubernetes",
    "lambda",
    "mongodb",
    "mysql",
    "netlify",
    "nextjs",
    "nginx",
    "openai",
    "postgres",
    "prisma",
    "prometheus",
    "rabbitmq",
    "railway",
    "redis",
    "render",
    "s3",
    "snowflake",
    "sqs",
    "stripe",
    "supabase",
    "terraform",
    "vercel",
];
/// Electron's ServiceMark inside the shared IconTile, with the same brand accent.
/// This inherits IconTile's documented approximation of the seeded chrome sheet.
pub fn service_mark_tile(name: &str, size: f32, mark_size: f32, _cx: &App) -> AnyElement {
    let mut tile = crate::icon_tile::IconTile::new(Icon::Cloud).size(size, mark_size);
    if let Some(color) = brand_color(name) {
        tile = tile
            .svg_asset(format!("service-marks/{}.svg", key(name)))
            .tint(color);
    }
    tile.into_any_element()
}

/// `PropertyPicker`'s OptionTile is 20px with a 12px mark (`option-tile.css`);
/// a selected pill draws the supplied size directly, without the tile.
pub fn option_mark(name: &str, size: f32, cx: &App) -> AnyElement {
    if size == 20.0 {
        service_mark_tile(name, size, 12.0, cx)
    } else {
        service_mark(name, size, cx)
    }
}

/// A deferred static mark, for renderers that do not need an app context of their own.
#[derive(IntoElement)]
pub struct ServiceMark {
    name: SharedString,
    size: f32,
    mark_size: Option<f32>,
}
impl ServiceMark {
    pub fn new(name: impl Into<SharedString>, size: f32) -> Self {
        Self {
            name: name.into(),
            size,
            mark_size: None,
        }
    }
}
impl ServiceMark {
    pub fn tile(mut self, mark_size: f32) -> Self {
        self.mark_size = Some(mark_size);
        self
    }
}
impl gpui::RenderOnce for ServiceMark {
    fn render(self, _: &mut gpui::Window, cx: &mut App) -> impl IntoElement {
        match self.mark_size {
            Some(mark_size) => service_mark_tile(&self.name, self.size, mark_size, cx),
            None => service_mark(&self.name, self.size, cx),
        }
    }
}

/// Electron's `ICON_TILE_BRAND_ACCENTS`, shared by inline marks and chrome tiles.
fn brand_color(name: &str) -> Option<gpui::Rgba> {
    Some(match key(name) {
        "auth0" => rgb(0xF29A7A),
        "aws" => rgb(0xFFB84D),
        "azure" => rgb(0x6BB3F0),
        "bigquery" => rgb(0x8FB8F8),
        "clerk" => rgb(0xA994FF),
        "cloudflare" => rgb(0xF5A85A),
        "datadog" => rgb(0xB08FD9),
        "digitalocean" => rgb(0x66B3FF),
        "docker" => rgb(0x6BB8F5),
        "elasticsearch" => rgb(0x6EB8C9),
        "fly" => rgb(0xB49CFF),
        "gcp" => rgb(0x7EB3F5),
        "github" => rgb(0xEDEDED),
        "github_actions" => rgb(0x6BB3FF),
        "gitlab" => rgb(0xF5A06A),
        "grafana" => rgb(0xF5A05A),
        "graphql" => rgb(0xF08CC8),
        "kafka" => rgb(0xC8C8C8),
        "kubernetes" => rgb(0x7EA8F0),
        "lambda" => rgb(0xFFB84D),
        "mongodb" => rgb(0x7EC98A),
        "mysql" => rgb(0x7EAFD0),
        "netlify" => rgb(0x5ED3C6),
        "nextjs" => rgb(0xEDEDED),
        "nginx" => rgb(0x6EC99A),
        "openai" => rgb(0x6EC9B0),
        "postgres" => rgb(0x7EA8C9),
        "prisma" => rgb(0xB8C4D0),
        "prometheus" => rgb(0xF08A6A),
        "rabbitmq" => rgb(0xFF9966),
        "railway" => rgb(0xC8C8C8),
        "redis" => rgb(0xF08A80),
        "render" => rgb(0xD6D6D6),
        "s3" => rgb(0x8EC97A),
        "snowflake" => rgb(0x6EC8F0),
        "sqs" => rgb(0xFF8FB8),
        "stripe" => rgb(0xA99CFF),
        "supabase" => rgb(0x6ED9A8),
        "terraform" => rgb(0xB08FD9),
        "vercel" => rgb(0xEDEDED),
        _ => return None,
    })
}

/// A platform's brand mark, at the caller's exact size. No public API is replaced.
pub fn service_mark(name: &str, size: f32, cx: &App) -> AnyElement {
    let name = key(name);
    let Some(color) = brand_color(name) else {
        return icon_colored(Icon::Cloud, size, Theme::global(cx).color_text_muted);
    };
    svg()
        .path(SharedString::from(format!("service-marks/{name}.svg")))
        .size(px(size))
        .flex_none()
        .text_color(color)
        .into_any_element()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_electron_mark_has_an_embedded_asset() {
        assert_eq!(key("google-cloud"), "gcp");
        for name in ALL {
            assert!(asset(&format!("service-marks/{name}.svg")).is_some());
        }
        assert!(asset("service-marks/unknown.svg").is_none());
    }
}
