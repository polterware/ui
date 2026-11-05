//! The Composer whole (`composer.tsx`'s `Composer`): the one dialog a record is created or
//! edited in, on a [`modal_layer`](super::modal_layer).
//!
//! It looks like the Solid `Composer` on the [`composer_card`]: the title (18px semibold,
//! wrapping), then the description (13.5px secondary, at least `rows={3}`'s 60px, its own
//! lines kept), the labelled fields ([`ComposerSpec::field`], such as a target date), the
//! [`PropertyPills`] 4px lower, and the Why field, every field a `.pw-composer__field` on the
//! dialog's own surface, 10px apart. The footer (`.pw-dialog-footer`) holds Create more
//! (a small [`Switch`] and its 12.5px secondary label, 8px apart) and a quiet action, 12px
//! apart, then the submit, together at its end. Every text button wears the texture face, as
//! the app's `TextureFaceProvider` sets it: the submit the primary tone (the info blue) and the
//! action the neutral one, on a small texture button's 6px corners. It fades in and out with the scrim on [`DialogPresence`]'s motion.
//!
//! It behaves like gpui-kit's `Dialog` holding a form: Enter in the title moves into the
//! description (or submits a Composer without one), Cmd+Enter submits from anywhere in it, and
//! Escape, Close, or a press on the scrim closes it unless a write is under way. Submit asks
//! the host with [`ComposerEvent::Submit`] and waits for [`Composer::finish`]: a write that
//! failed keeps the draft, and one that went through closes the Composer, or clears it for
//! the next record while Create more is on. The host owns every write and the properties,
//! which it hears of through [`ComposerEvent::Property`] and sets back.

use gpui::{
    AnyElement, App, AppContext as _, Context, Entity, EventEmitter, Focusable, IntoElement,
    KeyDownEvent, Render, SharedString, Window, div, prelude::*, px,
};

use super::composer::{composer_card, dialog_footer, solid_composer_field};
use super::dialog::DialogPresence;
use super::property_pills::{Property, PropertyPills, PropertyPillsEvent};
use super::shell::modal_layer;
use crate::Dismiss;
use crate::button::{Button, ButtonKind, ButtonSize};
use crate::switch::Switch;
use crate::text_input::{self, TextInput};
use crate::theme::Theme;

/// The description's least height (`min-height: 60px`, `rows={3}`).
const DESCRIPTION_MIN_H: f32 = 60.0;
/// A small texture button's corners (`rounded-[6px]`), the footer's buttons in the dialog.
const BUTTON_RADIUS: f32 = 6.0;

/// What a person typed; the host adds the properties it owns.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ComposerDraft {
    pub title: String,
    pub description: String,
    pub why: String,
    /// The labelled fields, in the spec's order.
    pub fields: Vec<String>,
}

/// What a Composer opens as: its words, which parts it shows, and the values it starts from
/// (`ComposerProps`).
#[derive(Clone, Debug)]
pub struct ComposerSpec {
    title_placeholder: SharedString,
    description_placeholder: Option<SharedString>,
    why_placeholder: Option<SharedString>,
    submit_label: SharedString,
    create_more: bool,
    action: Option<SharedString>,
    initial: ComposerDraft,
    /// Each labelled field's label and placeholder (`ComposerField`).
    fields: Vec<(SharedString, SharedString)>,
}

impl ComposerSpec {
    /// A Composer with only its title, `submit_label` on its submit, and Create more.
    pub fn new(
        title_placeholder: impl Into<SharedString>,
        submit_label: impl Into<SharedString>,
    ) -> Self {
        Self {
            title_placeholder: title_placeholder.into(),
            description_placeholder: None,
            why_placeholder: None,
            submit_label: submit_label.into(),
            create_more: true,
            action: None,
            initial: ComposerDraft::default(),
            fields: Vec::new(),
        }
    }

    /// A labelled text field under the description (`ComposerField`), such as a target date;
    /// its value starts from the initial draft's `fields` and comes back in the draft's.
    pub fn field(
        mut self,
        label: impl Into<SharedString>,
        placeholder: impl Into<SharedString>,
    ) -> Self {
        self.fields.push((label.into(), placeholder.into()));
        self
    }

    /// Shows the description, with this placeholder.
    pub fn description(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.description_placeholder = Some(placeholder.into());
        self
    }

    /// Shows the Why field, with this placeholder.
    pub fn why(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.why_placeholder = Some(placeholder.into());
        self
    }

    /// An edit: no Create more.
    pub fn edit(mut self) -> Self {
        self.create_more = false;
        self
    }

    /// A quiet action at the start of the footer, such as Remove rule: see
    /// [`ComposerEvent::Action`].
    pub fn action(mut self, label: impl Into<SharedString>) -> Self {
        self.action = Some(label.into());
        self
    }

    /// The values the fields start from, such as a record being edited.
    pub fn initial(mut self, initial: ComposerDraft) -> Self {
        self.initial = initial;
        self
    }
}

/// What the Composer tells its host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ComposerEvent {
    /// Write this draft, then answer with [`Composer::finish`]; the Composer waits, busy.
    Submit(ComposerDraft),
    /// The property with this id now holds these values; set them back with
    /// [`Composer::set_properties`].
    Property {
        id: SharedString,
        values: Vec<SharedString>,
    },
    /// The footer's action was clicked.
    Action,
    /// It closed: Close, Escape, the scrim, or a write that went through.
    Closed,
}

pub struct Composer {
    spec: ComposerSpec,
    title: Entity<TextInput>,
    description: Entity<TextInput>,
    why: Entity<TextInput>,
    /// The spec's labelled fields, made as it opens.
    fields: Vec<Entity<TextInput>>,
    pills: Entity<PropertyPills>,
    more: Entity<Switch>,
    presence: DialogPresence,
    busy: bool,
    /// Draws the card where it is declared, open and still, instead of on a modal layer.
    inline: bool,
    /// The title takes the focus on the next frame: the next record after Create more.
    refocus: bool,
}

impl EventEmitter<ComposerEvent> for Composer {}

impl Composer {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let view = cx.entity().downgrade();
        let title = cx.new(|cx| {
            TextInput::new(cx)
                .bare()
                .title()
                .wrap()
                .on_submit(move |_, window, cx| {
                    let view = view.clone();
                    // After the field's own update, which is still running.
                    window.defer(cx, move |window, cx| {
                        let _ = view.update(cx, |this, cx| this.title_entered(window, cx));
                    });
                })
        });
        let description = cx.new(|cx| TextInput::new(cx).description().multiline());
        let why = cx.new(|cx| TextInput::new(cx).inline());
        for field in [&title, &description, &why] {
            cx.observe(field, |_, _, cx| cx.notify()).detach();
        }
        let pills = cx.new(PropertyPills::new);
        cx.observe(&pills, |_, _, cx| cx.notify()).detach();
        cx.subscribe(&pills, |_, _, event: &PropertyPillsEvent, cx| {
            let PropertyPillsEvent::Changed { id, values } = event;
            cx.emit(ComposerEvent::Property {
                id: id.clone(),
                values: values.clone(),
            });
        })
        .detach();
        let more = cx.new(|cx| Switch::new(false, cx).small());
        cx.observe(&more, |_, _, cx| cx.notify()).detach();
        Self {
            spec: ComposerSpec::new("", ""),
            title,
            description,
            why,
            fields: Vec::new(),
            pills,
            more,
            presence: DialogPresence::new(),
            busy: false,
            inline: false,
            refocus: false,
        }
    }

    /// Draws it where it is declared, open and at rest, rather than over the window: a
    /// capture's Composer.
    pub fn inline(mut self) -> Self {
        self.inline = true;
        self.presence = DialogPresence::shown();
        self
    }

    /// Opens it as `spec` says, with `properties`, the fields set to its initial values and
    /// the title focused.
    pub fn open(
        &mut self,
        spec: ComposerSpec,
        properties: Vec<Property>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let initial = spec.initial.clone();
        let fields = [
            (&self.title, spec.title_placeholder.clone(), initial.title),
            (
                &self.description,
                spec.description_placeholder.clone().unwrap_or_default(),
                initial.description,
            ),
            (
                &self.why,
                spec.why_placeholder.clone().unwrap_or_default(),
                initial.why,
            ),
        ];
        for (field, placeholder, text) in fields {
            field.update(cx, |field, cx| {
                field.set_placeholder(placeholder);
                field.set_text(text, cx);
            });
        }
        self.fields = spec
            .fields
            .iter()
            .enumerate()
            .map(|(index, (_, placeholder))| {
                let text = initial_field(&spec.initial, index);
                let placeholder = placeholder.clone();
                let field = cx.new(|cx| {
                    let mut field = TextInput::new(cx).inline();
                    field.set_placeholder(placeholder);
                    field.set_text(text, cx);
                    field
                });
                cx.observe(&field, |_, _, cx| cx.notify()).detach();
                field
            })
            .collect();
        self.spec = spec;
        self.busy = false;
        self.set_properties(properties, cx);
        self.presence.set_open(true);
        self.title.focus_handle(cx).focus(window);
        cx.notify();
    }

    /// The property pills, as the host holds them now.
    pub fn set_properties(&mut self, properties: Vec<Property>, cx: &mut Context<Self>) {
        self.pills
            .update(cx, |pills, cx| pills.set_properties(properties, cx));
    }

    pub fn is_open(&self) -> bool {
        self.presence.is_open()
    }

    /// Whether a submit waits for its write.
    pub fn is_busy(&self) -> bool {
        self.busy
    }

    /// Closes it, whatever is under way, and says so with [`ComposerEvent::Closed`].
    pub fn close(&mut self, cx: &mut Context<Self>) {
        if self.presence.set_open(false) {
            self.busy = false;
            cx.emit(ComposerEvent::Closed);
            cx.notify();
        }
    }

    /// Answers a [`ComposerEvent::Submit`]: `written` false keeps the draft to try again;
    /// true closes it, or clears the fields for the next record while Create more is on.
    pub fn finish(&mut self, written: bool, cx: &mut Context<Self>) {
        self.busy = false;
        if written {
            if self.spec.create_more && self.more.read(cx).is_on() {
                for field in [&self.title, &self.description, &self.why]
                    .into_iter()
                    .chain(&self.fields)
                {
                    field.update(cx, |field, cx| field.set_text("", cx));
                }
                self.refocus = true;
            } else {
                self.close(cx);
            }
        }
        cx.notify();
    }

    /// The draft as typed, trimmed.
    pub fn draft(&self, cx: &App) -> ComposerDraft {
        let text = |field: &Entity<TextInput>| field.read(cx).text().trim().to_string();
        ComposerDraft {
            title: text(&self.title),
            description: if self.spec.description_placeholder.is_some() {
                text(&self.description)
            } else {
                String::new()
            },
            why: if self.spec.why_placeholder.is_some() {
                text(&self.why)
            } else {
                String::new()
            },
            fields: self.fields.iter().map(text).collect(),
        }
    }

    fn can_submit(&self, cx: &App) -> bool {
        !self.busy && !self.title.read(cx).text().trim().is_empty()
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        if !self.can_submit(cx) {
            return;
        }
        self.busy = true;
        cx.emit(ComposerEvent::Submit(self.draft(cx)));
        cx.notify();
    }

    /// Close, Escape, or the scrim: nothing while a write is under way.
    fn dismiss(&mut self, cx: &mut Context<Self>) {
        if !self.busy {
            self.close(cx);
        }
    }

    fn title_entered(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.spec.description_placeholder.is_some() {
            self.description.focus_handle(cx).focus(window);
        } else {
            self.submit(cx);
        }
    }

    fn create_more(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = *Theme::global(cx);
        let more = self.more.clone();
        div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .text_size(px(12.5))
            .text_color(theme.color_text_secondary)
            .child(self.more.clone())
            .child(
                div()
                    .id("composer-create-more")
                    .cursor_pointer()
                    .child("Create more")
                    .on_click(move |_, window, cx| {
                        more.update(cx, |more, cx| more.toggle(window, cx));
                    }),
            )
            .into_any_element()
    }
}

impl Render for Composer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.presence.is_present() {
            return div().into_any_element();
        }
        if std::mem::take(&mut self.refocus) {
            self.title.focus_handle(cx).focus(window);
        }
        let pose = self.presence.frame(window);
        let theme = *Theme::global(cx);
        let view = cx.entity().downgrade();
        let mut start = Vec::new();
        if self.spec.create_more {
            start.push(self.create_more(cx));
        }
        if let Some(label) = self.spec.action.clone() {
            let view = view.clone();
            start.push(
                Button::new("composer-action", ButtonKind::Secondary)
                    .size(ButtonSize::Sm)
                    .radius(BUTTON_RADIUS)
                    .label(label)
                    .disabled(self.busy)
                    .on_click(move |_, cx| {
                        let _ = view.update(cx, |_, cx| cx.emit(ComposerEvent::Action));
                    })
                    .into_any_element(),
            );
        }
        let submit = Button::new("composer-submit", ButtonKind::Primary)
            .size(ButtonSize::Sm)
            .radius(BUTTON_RADIUS)
            .label(self.spec.submit_label.clone())
            .disabled(!self.can_submit(cx))
            .on_click({
                let view = view.clone();
                move |_, cx| {
                    let _ = view.update(cx, |this, cx| this.submit(cx));
                }
            })
            .into_any_element();
        let close: crate::Handler = {
            let view = view.clone();
            std::rc::Rc::new(move |_, cx: &mut App| {
                let _ = view.update(cx, |this, cx| this.dismiss(cx));
            })
        };
        let mut card = composer_card(self.spec.submit_label.clone(), pose.opacity, 0.0, &theme)
            .root_scaled()
            .on_close(close.clone())
            .child(self.title.clone());
        if self.spec.description_placeholder.is_some() {
            let description = self.description.clone();
            card = card.child(
                div()
                    .id("composer-description")
                    .min_h(px(DESCRIPTION_MIN_H))
                    .cursor_text()
                    // A press under the lines still lands in the field, as a textarea's does.
                    .on_click(move |_, window, cx| {
                        description.focus_handle(cx).focus(window);
                    })
                    .child(self.description.clone()),
            );
        }
        for ((label, _), field) in self.spec.fields.iter().zip(&self.fields) {
            card = card.child(solid_composer_field(label.clone(), field.clone(), &theme));
        }
        if !self.pills.read(cx).properties().is_empty() {
            card = card.child(div().pt(px(4.0)).child(self.pills.clone()));
        }
        if self.spec.why_placeholder.is_some() {
            card = card.child(solid_composer_field("Why", self.why.clone(), &theme));
        }
        let card = card
            .footer(dialog_footer(start, [submit], &theme))
            .overlay(self.pills.read(cx).overlay(cx));
        let body = div()
            .key_context("Composer")
            .on_action(cx.listener(|this, _: &Dismiss, _, cx| this.dismiss(cx)))
            // Escape in a field that has nothing of its own to cancel.
            .on_action(cx.listener(|this, _: &text_input::Cancel, _, cx| this.dismiss(cx)))
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                let keys = &event.keystroke;
                if keys.key == "enter" && (keys.modifiers.platform || keys.modifiers.control) {
                    cx.stop_propagation();
                    this.submit(cx);
                }
            }))
            .child(card);
        if self.inline {
            return body.into_any_element();
        }
        modal_layer(pose.opacity, self.presence.is_open(), close, body, &theme)
    }
}

/// The initial text of labelled field `index`.
fn initial_field(initial: &ComposerDraft, index: usize) -> String {
    initial.fields.get(index).cloned().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use gpui::{Entity, TestAppContext, VisualTestContext};

    use super::{Composer, ComposerDraft, ComposerEvent, ComposerSpec};
    use crate::test_support::draw;

    type Events = Rc<RefCell<Vec<ComposerEvent>>>;

    /// A Composer opened as `spec` says, and the events it sent.
    fn composer(
        cx: &mut TestAppContext,
        spec: ComposerSpec,
    ) -> (Entity<Composer>, Events, &mut VisualTestContext) {
        cx.update(crate::init);
        let (composer, cx) = cx.add_window_view(|_, cx| Composer::new(cx));
        let events: Events = Rc::default();
        let sink = events.clone();
        cx.update(|window, cx| {
            cx.subscribe(&composer, move |_, event: &ComposerEvent, _| {
                sink.borrow_mut().push(event.clone());
            })
            .detach();
            composer.update(cx, |composer, cx| {
                composer.open(spec, Vec::new(), window, cx)
            });
        });
        draw(cx);
        (composer, events, cx)
    }

    fn submits(events: &Events) -> Vec<ComposerDraft> {
        events
            .borrow()
            .iter()
            .filter_map(|event| match event {
                ComposerEvent::Submit(draft) => Some(draft.clone()),
                _ => None,
            })
            .collect()
    }

    #[gpui::test]
    fn enter_moves_into_the_description_and_cmd_enter_submits_the_draft(cx: &mut TestAppContext) {
        let spec = ComposerSpec::new("Doc title", "Create doc")
            .description("Write in Markdown…")
            .why("Optional");
        let (composer, events, cx) = composer(cx, spec);
        cx.simulate_input(" Runbook ");
        cx.simulate_keystrokes("enter");
        cx.simulate_input("one");
        cx.simulate_keystrokes("enter");
        cx.simulate_input("two");
        assert!(submits(&events).is_empty(), "Enter submits nothing");
        cx.simulate_keystrokes("cmd-enter");
        assert_eq!(
            submits(&events),
            [ComposerDraft {
                title: "Runbook".into(),
                description: "one\ntwo".into(),
                why: String::new(),
                fields: Vec::new(),
            }]
        );
        // Busy, it neither submits again nor closes.
        cx.simulate_keystrokes("cmd-enter escape");
        assert_eq!(submits(&events).len(), 1);
        assert!(cx.update(|_, cx| composer.read(cx).is_open()));
        // A failed write keeps the draft; one that went through closes it.
        cx.update(|_, cx| composer.update(cx, |c, cx| c.finish(false, cx)));
        assert_eq!(
            cx.update(|_, cx| composer.read(cx).draft(cx).title),
            "Runbook"
        );
        cx.update(|_, cx| composer.update(cx, |c, cx| c.finish(true, cx)));
        assert!(!cx.update(|_, cx| composer.read(cx).is_open()));
        assert_eq!(events.borrow().last(), Some(&ComposerEvent::Closed));
    }

    #[gpui::test]
    fn a_labelled_field_starts_from_the_draft_and_comes_back_in_it(cx: &mut TestAppContext) {
        let spec = ComposerSpec::new("Milestone name", "Create milestone")
            .field("Target date", "YYYY-MM-DD")
            .initial(ComposerDraft {
                fields: vec!["2026-12-01".into()],
                ..ComposerDraft::default()
            });
        let (_, events, cx) = composer(cx, spec);
        cx.simulate_input("Beta");
        cx.simulate_keystrokes("cmd-enter");
        assert_eq!(submits(&events)[0].fields, ["2026-12-01".to_string()]);
    }

    #[gpui::test]
    fn a_composer_with_only_a_title_submits_on_enter_and_escape_closes_it(cx: &mut TestAppContext) {
        let spec = ComposerSpec::new("The API returns only declared fields", "Save rule")
            .edit()
            .initial(ComposerDraft {
                title: "Keep one writer".into(),
                ..ComposerDraft::default()
            });
        let (composer, events, cx) = composer(cx, spec);
        cx.simulate_keystrokes("enter");
        assert_eq!(submits(&events)[0].title, "Keep one writer");
        cx.update(|_, cx| composer.update(cx, |c, cx| c.finish(false, cx)));
        cx.simulate_keystrokes("escape");
        assert!(!cx.update(|_, cx| composer.read(cx).is_open()));
        assert_eq!(events.borrow().last(), Some(&ComposerEvent::Closed));
    }

    #[gpui::test]
    fn create_more_clears_the_fields_for_the_next_record(cx: &mut TestAppContext) {
        let spec = ComposerSpec::new("Decision title", "Create decision");
        let (composer, events, cx) = composer(cx, spec);
        cx.update(|window, cx| {
            let more = composer.read(cx).more.clone();
            more.update(cx, |more, cx| more.toggle(window, cx));
        });
        cx.simulate_input("First");
        cx.simulate_keystrokes("enter");
        assert_eq!(submits(&events).len(), 1);
        cx.update(|_, cx| composer.update(cx, |c, cx| c.finish(true, cx)));
        assert!(cx.update(|_, cx| composer.read(cx).is_open()));
        assert_eq!(cx.update(|_, cx| composer.read(cx).draft(cx).title), "");
        // An empty title submits nothing.
        cx.simulate_keystrokes("cmd-enter");
        assert_eq!(submits(&events).len(), 1);
    }
}
