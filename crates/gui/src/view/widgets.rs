//! Small building blocks shared by the screens.
//!
//! White panels hold real objects (files, folders, runs). Settings sit in
//! unboxed sections. Buttons take keyboard focus, show a ring when focused
//! from the keyboard, and activate with Enter or Space.
use super::palette::*;
use super::WizardView;
use crate::selection::Tick;
use gpui::{
    div, prelude::*, px, relative, rgb, Context, Div, ElementId, FocusHandle, FontWeight,
    KeyDownEvent, SharedString, Stateful, Window,
};
use std::rc::Rc;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ButtonKind {
    Primary,
    Secondary,
    /// Stops or removes something.
    Danger,
    DangerSolid,
    /// Text-only, for small inline actions.
    Quiet,
}

type Handler = dyn Fn(&mut WizardView, &mut Window, &mut Context<WizardView>);

/// Makes an element a keyboard-reachable control with a visible focus ring.
pub fn focusable(element: Stateful<Div>, focus: &FocusHandle) -> Stateful<Div> {
    element
        .track_focus(focus)
        .tab_index(0)
        .border_2()
        .border_color(gpui::transparent_black())
        .focus_visible(|s| s.border_color(rgb(FOCUS)))
}

/// A button whose debug selector is its name, for headless tests.
pub fn button(
    view: &WizardView,
    name: &'static str,
    label: impl Into<SharedString>,
    kind: ButtonKind,
    enabled: bool,
    cx: &Context<WizardView>,
    on_click: impl Fn(&mut WizardView, &mut Window, &mut Context<WizardView>) + 'static,
) -> Stateful<Div> {
    button_with(
        view,
        name.into(),
        name.to_string(),
        label,
        kind,
        enabled,
        cx,
        on_click,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn button_with(
    view: &WizardView,
    id: ElementId,
    selector: String,
    label: impl Into<SharedString>,
    kind: ButtonKind,
    enabled: bool,
    cx: &Context<WizardView>,
    on_click: impl Fn(&mut WizardView, &mut Window, &mut Context<WizardView>) + 'static,
) -> Stateful<Div> {
    let (bg, fg, line) = match kind {
        ButtonKind::Primary => (TEAL, PAPER, TEAL),
        ButtonKind::Secondary => (PAPER, INK, HAIRLINE_STRONG),
        ButtonKind::Danger => (PAPER, FAULT, 0xe9c3bf),
        ButtonKind::DangerSolid => (FAULT, PAPER, FAULT),
        ButtonKind::Quiet => (MIST, TEAL_TEXT, MIST),
    };
    let focus = view.focus_for(&selector, cx);
    let handler: Rc<Handler> = Rc::new(on_click);
    let key_handler = handler.clone();
    // The outer box only draws the focus ring, so focusing never moves layout.
    let face = div()
        .flex()
        .items_center()
        .justify_center()
        .whitespace_nowrap()
        .rounded(px(5.))
        .border_1()
        .map(|b| match kind {
            ButtonKind::Quiet => b.px(px(6.)).py(px(3.)).font_weight(FontWeight::SEMIBOLD),
            ButtonKind::Primary | ButtonKind::DangerSolid => {
                b.px(px(16.)).py(px(6.)).font_weight(FontWeight::SEMIBOLD)
            }
            _ => b.px(px(16.)).py(px(6.)),
        })
        .map(|b| {
            if enabled {
                b.bg(rgb(bg))
                    .border_color(rgb(line))
                    .text_color(rgb(fg))
                    .group_hover("button", move |s| s.bg(rgb(hover(kind))))
            } else {
                let disabled = if kind == ButtonKind::Quiet {
                    MIST
                } else {
                    DISABLED_BG
                };
                b.bg(rgb(disabled))
                    .border_color(rgb(disabled))
                    .text_color(rgb(DISABLED_TEXT))
            }
        })
        .child(label.into());
    let base = div()
        .id(id)
        .debug_selector(move || selector)
        .group("button")
        .flex_none()
        .rounded(px(7.))
        .child(face);
    if enabled {
        focusable(base, &focus)
            .cursor_pointer()
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                    cx.stop_propagation();
                    key_handler(this, window, cx);
                }
            }))
            .on_click(cx.listener(move |this, _, window, cx| handler(this, window, cx)))
    } else {
        base.border_2().border_color(gpui::transparent_black())
    }
}

fn hover(kind: ButtonKind) -> u32 {
    match kind {
        ButtonKind::Primary => TEAL_DEEP,
        ButtonKind::Secondary => 0xe9edf1,
        ButtonKind::Danger => 0xfbeceb,
        ButtonKind::DangerSolid => 0x8f1c13,
        ButtonKind::Quiet => 0xe6ebef,
    }
}

/// A white panel for a real object: a file, the folder tree, a run.
pub fn panel() -> Div {
    div()
        .bg(rgb(PAPER))
        .border_1()
        .border_color(rgb(HAIRLINE))
        .rounded(px(10.))
}

/// An unboxed settings section: a heading, a sentence, then its controls.
pub fn section(title: impl Into<SharedString>, help: impl Into<SharedString>) -> Div {
    let help: SharedString = help.into();
    div()
        .flex()
        .flex_col()
        .gap(px(4.))
        .pt(px(18.))
        .pb(px(20.))
        .border_t_1()
        .border_color(rgb(HAIRLINE))
        .child(heading(title))
        .when(!help.is_empty(), |d| d.child(secondary(help)))
}

pub fn heading(text: impl Into<SharedString>) -> Div {
    div()
        .text_size(px(17.))
        .font_weight(FontWeight::SEMIBOLD)
        .child(text.into())
}

pub fn secondary(text: impl Into<SharedString>) -> Div {
    div()
        .text_size(px(13.))
        .line_height(px(19.))
        .text_color(rgb(PEWTER))
        .child(text.into())
}

/// File paths and property tags are identifiers, so they are set in mono.
pub fn identifier(text: impl Into<SharedString>) -> Div {
    div()
        .font_family(SharedString::from("Cascadia Mono"))
        .text_size(px(12.5))
        .child(text.into())
}

#[derive(Clone, Copy)]
pub enum NoteKind {
    Info,
    Warn,
}

pub fn note(kind: NoteKind, title: impl Into<SharedString>, body: impl Into<SharedString>) -> Div {
    let (bg, line, fg) = match kind {
        NoteKind::Info => (0xe8eef3, 0xd3dde6, INK),
        NoteKind::Warn => (AMBER_BG, 0xf0d9a8, AMBER),
    };
    let title: SharedString = title.into();
    let body: SharedString = body.into();
    div()
        .flex()
        .flex_col()
        .gap(px(2.))
        .rounded(px(8.))
        .border_l_4()
        .border_color(rgb(line))
        .px(px(14.))
        .py(px(10.))
        .bg(rgb(bg))
        .text_color(rgb(fg))
        .text_size(px(13.))
        .line_height(px(19.))
        .when(!title.is_empty(), |d| {
            d.child(div().font_weight(FontWeight::SEMIBOLD).child(title))
        })
        .when(!body.is_empty(), |d| d.child(body))
}

/// The one loud element: the engine's completeness statement, word for word,
/// with its first sentence set as a headline.
pub fn completeness_band(statement: &str) -> Div {
    let (lead, rest) = match statement.find(". ") {
        Some(end) => (&statement[..=end], statement[end + 2..].trim()),
        None => (statement, ""),
    };
    div()
        .rounded(px(10.))
        .bg(rgb(INK))
        .text_color(rgb(PAPER))
        .px(px(22.))
        .py(px(18.))
        .flex()
        .flex_col()
        .gap(px(6.))
        .child(
            div()
                .font_family(SharedString::from(DISPLAY_FONT))
                .text_size(px(22.))
                .line_height(px(28.))
                .font_weight(FontWeight::SEMIBOLD)
                .child(lead.to_string()),
        )
        .when(!rest.is_empty(), |d| {
            d.child(
                div()
                    .max_w(px(640.))
                    .text_size(px(14.))
                    .line_height(px(21.))
                    .text_color(rgb(0xd5dce3))
                    .child(rest.to_string()),
            )
        })
        .child(
            div()
                .max_w(px(640.))
                .text_size(px(14.))
                .line_height(px(21.))
                .text_color(rgb(0xd5dce3))
                .child("Before you delete anything from the mailbox, check that the PST holds the folders and dates you expect."),
        )
}

#[derive(Clone, Copy)]
pub enum ChipKind {
    Ok,
    Warn,
    Neutral,
    Running,
    Bad,
}

pub fn chip(kind: ChipKind, text: impl Into<SharedString>) -> Div {
    let (bg, fg) = match kind {
        ChipKind::Ok => (TEAL_TINT, TEAL_TEXT),
        ChipKind::Warn => (AMBER_BG, AMBER),
        ChipKind::Neutral => (0xe8ebee, PEWTER),
        ChipKind::Running => (0xe3ecf9, 0x24508f),
        ChipKind::Bad => (0xfbe9e7, FAULT),
    };
    div()
        .flex_none()
        .whitespace_nowrap()
        .px(px(9.))
        .py(px(2.))
        .rounded(px(5.))
        .bg(rgb(bg))
        .text_color(rgb(fg))
        .text_size(px(12.5))
        .font_weight(FontWeight::SEMIBOLD)
        .child(text.into())
}

pub fn bar(fraction: f64, height: f32) -> Div {
    div()
        .w_full()
        .h(px(height))
        .rounded(px(height / 2.))
        .bg(rgb(0xdfe4e9))
        .overflow_hidden()
        .child(
            div()
                .h_full()
                .bg(rgb(TEAL))
                .w(relative(fraction.clamp(0.0, 1.0) as f32)),
        )
}

pub fn checkbox(tick: Tick) -> Div {
    let on = tick != Tick::Off;
    div()
        .size(px(16.))
        .flex_none()
        .rounded(px(4.))
        .border_1()
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(11.))
        .font_weight(FontWeight::BOLD)
        .map(|d| {
            if on {
                d.bg(rgb(TEAL))
                    .border_color(rgb(TEAL))
                    .text_color(rgb(PAPER))
            } else {
                d.bg(rgb(PAPER)).border_color(rgb(0x8d97a3))
            }
        })
        .child(match tick {
            Tick::On => "✓",
            Tick::Mixed => "–",
            Tick::Off => "",
        })
}

pub fn toggle(on: bool) -> Div {
    div()
        .w(px(40.))
        .h(px(22.))
        .flex_none()
        .rounded(px(11.))
        .p(px(3.))
        .flex()
        .bg(rgb(if on { TEAL } else { 0xb9c2cb }))
        .when(on, |d| d.justify_end())
        .child(div().size(px(16.)).rounded(px(8.)).bg(rgb(PAPER)))
}

pub fn copy_mode_note() -> Div {
    div()
        .text_size(px(12.5))
        .line_height(px(18.))
        .text_color(rgb(PEWTER))
        .child(
            div()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(INK))
                .child("Copy mode"),
        )
        .child("PstOff only reads your OST files and writes new PST files. It never changes an OST and never connects to the mailbox.")
}

pub fn list_row() -> Div {
    div()
        .flex()
        .gap(px(12.))
        .py(px(8.))
        .border_t_1()
        .border_color(rgb(HAIRLINE))
        .text_size(px(13.))
}
