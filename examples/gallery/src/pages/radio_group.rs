//! `radio-group.docs.tsx`: three reviewers in a `w-64` column, none picked. The GPUI group
//! has no trailing slot, so Edsger's "away" is left out.

use std::rc::Rc;

use gpui::{AnyElement, App, IntoElement, Window, div, prelude::*, px};
use polterware_ui::{RadioOption, radio_group};

use super::super::Example;

fn pick_one(_: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let options: Vec<RadioOption> = ["Ada", "Grace", "Edsger"]
        .into_iter()
        .map(|label| RadioOption {
            label: label.into(),
        })
        .collect();
    vec![
        div()
            .w(px(256.0))
            .child(radio_group(
                "gallery-reviewer",
                &options,
                usize::MAX,
                Rc::new(|_, _, _| {}),
                cx,
            ))
            .into_any_element(),
    ]
}

pub fn examples() -> Vec<Example> {
    vec![Example {
        slug: "radiogroup",
        title: "Pick one",
        height: 152.25,
        render: pick_one,
    }]
}
