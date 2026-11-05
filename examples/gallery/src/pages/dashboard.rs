//! The metrics dashboard: spark charts (a live series, a fixed percentage range with its area,
//! a short series filling from the right), readings in their grid, a headline, meters, the
//! rows that pick a series, dashboard cards packed on the grid (drag a header or a grip to
//! move a card; the arrow keys move the one whose grip has focus), and a table row's context
//! menu.

use std::rc::Rc;

use gpui::{AnyElement, App, IntoElement, SharedString, Window, deferred, div, prelude::*, px};
use polterware_ui::theme::Theme;
use polterware_ui::{
    Column, ContextMenu, DashboardCard, DashboardLayout, DataTable, GridCard, Icon, Level,
    MenuItem, Meter, PickRow, Reading, SparkChart, dashboard_grid, dashboard_grip, headline,
    meters, readings,
};

use super::super::Example;

fn wave(len: usize, scale: f32) -> Vec<f32> {
    (0..len)
        .map(|index| {
            let t = index as f32 / 7.0;
            ((t.sin() * 0.5 + 0.5) * 0.7 + (t * 2.3).cos().abs() * 0.3) * scale
        })
        .collect()
}

fn charts(_: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let theme = *Theme::global(cx);
    vec![
        div()
            .w(px(420.0))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(
                SparkChart::new("gallery-spark-rate", wave(120, 3_400_000.0))
                    .slots(120)
                    .height(80.0)
                    .color(theme.chart_cyan)
                    .label("Down"),
            )
            .child(
                SparkChart::new("gallery-spark-percent", wave(120, 70.0))
                    .slots(120)
                    .domain(0.0, 100.0)
                    .color(theme.chart_red)
                    .area()
                    .format(|value| format!("{value:.0}%").into()),
            )
            .into_any_element(),
        div()
            .w(px(380.0))
            .child(
                SparkChart::new("gallery-spark-short", wave(18, 900.0))
                    .slots(120)
                    .color(theme.chart_blue),
            )
            .into_any_element(),
    ]
}

fn tiles(_: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let theme = *Theme::global(cx);
    let temp = |celsius: f32, label: &str| {
        Reading::new(format!("{celsius:.1}"), label.to_string())
            .unit("°C")
            .bar(
                celsius / 100.0,
                Level::of(celsius as f64, 65.0, 80.0).color(&theme),
            )
    };
    vec![
        div()
            .w(px(360.0))
            .child(readings(
                vec![
                    temp(42.5, "thermal_zone0"),
                    temp(71.0, "thermal_zone1"),
                    temp(88.0, "GPU 0 temp"),
                    Reading::new("37", "GPU 0 util")
                        .unit("%")
                        .bar(0.37, theme.chart_cyan),
                    Reading::split("154.4 MB", "Swap of 2.0 GB"),
                    Reading::new("3d 4h", "Uptime"),
                ],
                None,
                360.0,
                cx,
            ))
            .into_any_element(),
        div()
            .w(px(220.0))
            .child(headline(
                "42%",
                "Load 1.20 / 0.98 / 0.75",
                Some((0.42, theme.chart_cyan)),
                cx,
            ))
            .into_any_element(),
    ]
}

fn compact(window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let theme = *Theme::global(cx);
    let cores = [12.0, 64.0, 91.0, 30.0, 7.0, 55.0, 88.0, 41.0];
    let picked = window.use_keyed_state("gallery-pick-row", cx, |_, _| 0usize);
    let current = *picked.read(cx);
    let rows = [
        ("en0", "1.2 MB/s", "84 KB/s"),
        ("utun3", "3 KB/s", "1 KB/s"),
    ];
    vec![
        div()
            .w(px(260.0))
            .child(meters(
                cores
                    .iter()
                    .enumerate()
                    .map(|(index, usage)| {
                        Meter::new(
                            format!("cpu{index}"),
                            format!("{usage:.0}%"),
                            usage / 100.0,
                            Level::of(*usage as f64, 60.0, 85.0).color(&theme),
                        )
                    })
                    .collect(),
                260.0,
                cx,
            ))
            .into_any_element(),
        div()
            .w(px(300.0))
            .flex()
            .flex_col()
            .gap(px(4.0))
            .children(rows.iter().enumerate().map(|(index, (name, down, up))| {
                let picked = picked.clone();
                PickRow::new(("gallery-pick", index))
                    .cells([name.to_string(), format!("Down {down}"), format!("Up {up}")])
                    .selected(index == current)
                    .on_click(std::rc::Rc::new(move |_, cx| {
                        picked.update(cx, |picked, _| *picked = index);
                    }))
            }))
            .into_any_element(),
    ]
}

/// A sample card: its key, title, icon, size in cells, and body.
type Sample = (&'static str, &'static str, Icon, (u16, u16), AnyElement);

fn grid(window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let theme = *Theme::global(cx);
    let layout = window.use_keyed_state("gallery-dashboard-layout", cx, |_, _| {
        DashboardLayout::new(["cpu", "memory", "pressure", "disk"])
    });
    let bodies: Vec<Sample> = vec![
        (
            "cpu",
            "CPU",
            Icon::Cpu,
            (1, 2),
            div()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .child(headline("42%", "Load 1.20 / 0.98 / 0.75", None, cx))
                .child(
                    SparkChart::new("gallery-card-cpu-spark", wave(120, 60.0))
                        .slots(120)
                        .domain(0.0, 100.0)
                        .color(theme.chart_red),
                )
                .into_any_element(),
        ),
        (
            "memory",
            "Memory",
            Icon::MemoryStick,
            (1, 1),
            headline(
                "63%",
                "10.1 GB / 16.0 GB",
                Some((0.63, theme.chart_amber)),
                cx,
            ),
        ),
        (
            "pressure",
            "Pressure",
            Icon::Gauge,
            (1, 1),
            headline("3%", "Highest stall: CPU", None, cx),
        ),
        (
            "disk",
            "Disks",
            Icon::HardDrive,
            (2, 1),
            div().child("/ · 87%").into_any_element(),
        ),
    ];
    let cards: Vec<GridCard> = bodies
        .into_iter()
        .map(|(key, title, icon, size, body)| {
            let grip = dashboard_grip("gallery-dashboard", key, window, cx);
            let card = DashboardCard::new(SharedString::from(format!("gallery-card-{key}")), title)
                .icon(icon)
                .grip(grip)
                .child(body);
            GridCard::new(key, size, card)
        })
        .collect();
    let saved = layout.read(cx).clone();
    vec![
        div()
            .w(px(872.0))
            .child(
                dashboard_grid("gallery-dashboard")
                    .guess(3)
                    .layout(&saved)
                    .cards(cards)
                    .on_change(Rc::new(move |next, _, cx| {
                        layout.update(cx, |layout, _| *layout = next);
                    }))
                    .build(window, cx),
            )
            .into_any_element(),
    ]
}

fn context(window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let menu = window.use_keyed_state("gallery-context-menu", cx, |_, cx| ContextMenu::new(cx));
    let marked = window.use_keyed_state("gallery-context-row", cx, |_, _| None::<usize>);
    let rows = [("42", "nginx"), ("77", "postgres"), ("108", "node")];
    let open_menu = menu.clone();
    let open_marked = marked.clone();
    let table = DataTable::new(
        "gallery-context-table",
        vec![Column::new("PID").width(72.0), Column::new("Command")],
        rows.len(),
        move |row, _, _| {
            let (pid, command) = rows[row.index];
            vec![
                div().child(pid).into_any_element(),
                div().child(command).into_any_element(),
            ]
        },
    )
    .row_height(36.0)
    .default_density()
    .marked(*marked.read(cx))
    .on_context(move |index, at, window, cx| {
        open_marked.update(cx, |row, _| *row = Some(index));
        let noop = std::rc::Rc::new(|_: &mut Window, _: &mut App| {});
        open_menu.update(cx, |menu, cx| {
            menu.open(
                at,
                vec![
                    MenuItem::action("Copy PID", Some(Icon::Copy), noop.clone()),
                    MenuItem::Separator,
                    MenuItem::action("Kill process", Some(Icon::Stop), noop),
                ],
                window,
                cx,
            );
        });
    });
    if !menu.read(cx).is_open(cx) && marked.read(cx).is_some() {
        marked.update(cx, |row, _| *row = None);
    }
    vec![
        div()
            .w(px(360.0))
            .h(px(170.0))
            .child(table)
            .into_any_element(),
        div().child("Right-click a row").into_any_element(),
        deferred(menu).into_any_element(),
    ]
}

pub fn examples() -> Vec<Example> {
    let example = |title, height, render| Example {
        slug: "dashboard",
        title,
        height,
        render,
    };
    vec![
        example("Spark charts", 260.0, charts),
        example("Readings and a headline", 200.0, tiles),
        example("Meters and the rows that pick a series", 200.0, compact),
        example("Dashboard cards on the packed grid", 470.0, grid),
        example("A table row's context menu", 230.0, context),
    ]
}
