use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;

use engine::attribution::selection_savings;
use engine::model::{RootKind, Root};
use gtk::prelude::*;
use gtk::{gio, glib};

use crate::clean::{self, PlanLink};
use crate::format;
use crate::rows::{self, Cell, Column, ColumnKind, Marker, Row};
use crate::state::{Measured, SharedState};
use crate::ui;

static COLUMNS: [Column; 6] = [
    Column {
        title: "",
        width: 28,
        align: gtk::Align::Center,
        kind: ColumnKind::Checkbox,
    },
    Column {
        title: "Root",
        width: 0,
        align: gtk::Align::Start,
        kind: ColumnKind::Text,
    },
    Column {
        title: "",
        width: 44,
        align: gtk::Align::Center,
        kind: ColumnKind::Icon,
    },
    Column {
        title: "Gen",
        width: 60,
        align: gtk::Align::End,
        kind: ColumnKind::Text,
    },
    Column {
        title: "Space taken",
        width: 120,
        align: gtk::Align::End,
        kind: ColumnKind::Text,
    },
    Column {
        title: "Freeable space",
        width: 150,
        align: gtk::Align::End,
        kind: ColumnKind::Text,
    },
];

fn kind_icon(kind: RootKind) -> (&'static str, &'static str) {
    match kind {
        RootKind::SystemCurrent => ("computer-symbolic", "SYS"),
        RootKind::BootedSystem => ("drive-harddisk-symbolic", "BOOT"),
        RootKind::SystemGeneration => ("document-open-recent-symbolic", "GEN"),
        RootKind::HomeManagerCurrent => ("user-home-symbolic", "HM"),
        RootKind::HomeManagerGeneration => ("user-home-symbolic", "HMG"),
        RootKind::Profile => ("avatar-default-symbolic", "PRF"),
        RootKind::ResultLink => ("folder-symbolic", "RES"),
        RootKind::Gcroot => ("link-symbolic", "GC"),
        RootKind::Other => ("dialog-question-symbolic", "?"),
    }
}

fn kind_cell(root: &Root) -> Cell {
    let (name, fallback) = kind_icon(root.kind);
    let mut tooltip = root.kind.label().to_string();
    if root.protection.is_protected() {
        tooltip.push_str(" · protected");
    }
    Cell::Icon {
        name,
        fallback,
        tooltip,
    }
}

pub struct RootsPanel {
    pub widget: gtk::Box,
    list: gtk::ListView,
    store: gio::ListStore,
    selected: RefCell<HashSet<u32>>,
    summary: gtk::Label,
    detail: gtk::Label,
    calculate: gtk::Button,
    clean: gtk::Button,
    busy: Cell<bool>,
    spinner: gtk::Spinner,
    window: gtk::Window,
    state: SharedState,
    rescan: RefCell<Option<Rc<dyn Fn()>>>,
}

impl RootsPanel {
    pub fn new(window: &gtk::Window, state: SharedState) -> Rc<Self> {
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 0);
        widget.set_hexpand(true);
        widget.set_vexpand(true);

        let (list, store) = rows::list_view(&COLUMNS);

        let summary = gtk::Label::new(Some("No scan yet"));
        summary.set_xalign(0.0);
        summary.set_wrap(true);
        summary.set_hexpand(true);

        let detail = gtk::Label::new(None);
        detail.set_xalign(0.0);
        detail.set_wrap(true);
        detail.add_css_class("dim-label");

        let calculate = gtk::Button::with_label("Calculate actual");
        calculate.set_sensitive(false);
        let clean = gtk::Button::with_label("Clean selected");
        clean.set_sensitive(false);
        clean.add_css_class("destructive-action");

        let clear = gtk::Button::with_label("Clear selection");
        clear.set_sensitive(false);

        let spinner = gtk::Spinner::new();
        spinner.set_visible(false);
        spinner.set_valign(gtk::Align::Center);

        let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        buttons.set_halign(gtk::Align::End);
        buttons.append(&spinner);
        buttons.append(&clear);
        buttons.append(&calculate);
        buttons.append(&clean);

        let footer = gtk::Box::new(gtk::Orientation::Vertical, 6);
        footer.set_margin_top(10);
        footer.set_margin_bottom(10);
        footer.set_margin_start(12);
        footer.set_margin_end(12);
        footer.append(&summary);
        footer.append(&detail);
        footer.append(&buttons);

        widget.append(&rows::header_row(&COLUMNS));
        widget.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        widget.append(&rows::scrolled(&list));
        widget.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        widget.append(&footer);

        let panel = Rc::new(RootsPanel {
            widget,
            list,
            store,
            selected: RefCell::new(HashSet::new()),
            summary,
            detail,
            calculate,
            clean,
            busy: Cell::new(false),
            spinner,
            window: window.clone(),
            state,
            rescan: RefCell::new(None),
        });

        panel.list.set_single_click_activate(true);
        let weak = Rc::downgrade(&panel);
        panel.list.connect_activate(move |list, position| {
            let Some(panel) = weak.upgrade() else {
                return;
            };
            let Some(model) = list.model() else {
                return;
            };
            let Some(object) = model.item(position).and_downcast::<glib::BoxedAnyObject>() else {
                return;
            };
            let row = object.borrow::<Row>().clone();
            panel.toggle(row.id);
        });

        let weak = Rc::downgrade(&panel);
        panel.calculate.connect_clicked(move |_| {
            if let Some(panel) = weak.upgrade() {
                panel.calculate_actual();
            }
        });

        let weak = Rc::downgrade(&panel);
        panel.clean.connect_clicked(move |_| {
            if let Some(panel) = weak.upgrade() {
                panel.confirm_clean();
            }
        });

        let weak = Rc::downgrade(&panel);
        clear.connect_clicked(move |_| {
            if let Some(panel) = weak.upgrade() {
                panel.selected.borrow_mut().clear();
                panel.state.last_actual.set(None);
                panel.rebuild();
            }
        });

        panel
    }

    pub fn set_rescan(&self, rescan: Rc<dyn Fn()>) {
        *self.rescan.borrow_mut() = Some(rescan);
    }

    fn set_busy(&self, busy: bool) {
        self.busy.set(busy);
        self.spinner.set_visible(busy);
        if busy {
            self.spinner.start();
        } else {
            self.spinner.stop();
        }
        self.calculate
            .set_label(if busy { "Calculating…" } else { "Calculate actual" });
    }

    fn toggle(self: &Rc<Self>, id: u32) {
        if self.busy.get() {
            return;
        }
        {
            let mut selected = self.selected.borrow_mut();
            if !selected.remove(&id) {
                selected.insert(id);
            }
        }
        self.state.last_actual.set(None);
        self.rebuild();
    }

    pub fn rebuild(self: &Rc<Self>) {
        let loaded = match self.state.loaded.borrow().clone() {
            Some(loaded) => loaded,
            None => {
                rows::replace_rows(&self.store, Vec::new());
                self.selected.borrow_mut().clear();
                self.update_summary();
                return;
            }
        };

        let index = &loaded.index;
        {
            let mut selected = self.selected.borrow_mut();
            selected.retain(|id| (*id as usize) < index.roots.len());
        }
        let selected = self.selected.borrow();

        let mut model_rows = Vec::with_capacity(index.roots.len());

        for (position, root) in index.roots.iter().enumerate() {
            let cells = vec![
                Cell::Checkbox(selected.contains(&(position as u32))),
                Cell::Text(root.label.clone()),
                kind_cell(root),
                Cell::Text(root.generation.map(|generation| generation.to_string()).unwrap_or_default()),
                Cell::Text(format::bytes(
                    index.analysis.closure_sizes.get(position).copied().unwrap_or(0),
                )),
                Cell::Text(format::bytes(
                    index.analysis.individual_sizes.get(position).copied().unwrap_or(0),
                )),
            ];
            model_rows.push(Row {
                id: position as u32,
                depth: 0,
                cells,
                marker: Marker::None,
            });
        }

        rows::replace_rows(&self.store, model_rows);
        drop(selected);
        self.update_summary();
    }

    fn selected_ids(&self) -> Vec<u32> {
        let mut selected: Vec<u32> = self.selected.borrow().iter().copied().collect();
        selected.sort_unstable();
        selected
    }

    fn update_summary(self: &Rc<Self>) {
        let selected = self.selected_ids();
        let loaded = self.state.loaded.borrow().clone();

        let Some(loaded) = loaded else {
            self.summary.set_label("No scan yet");
            self.detail.set_label("");
            self.calculate.set_sensitive(false);
            self.clean.set_sensitive(false);
            return;
        };

        if selected.is_empty() {
            self.summary
                .set_label("Tick gc roots to see how much space cleaning them would free");
            self.detail.set_label("");
            self.calculate.set_sensitive(false);
            self.clean.set_sensitive(false);
            return;
        }

        let blocked: Vec<String> = selected
            .iter()
            .filter_map(|index| loaded.index.roots.get(*index as usize))
            .filter(|root| root.protection.is_protected())
            .map(|root| format!("{} ({})", root.label, root.protection.reason()))
            .collect();

        if !blocked.is_empty() {
            self.summary.set_label(&format!(
                "Selection contains protected roots: {}",
                blocked.join(", ")
            ));
            self.detail
                .set_label("Untick the protected roots to continue");
            self.calculate.set_sensitive(false);
            self.clean.set_sensitive(false);
            return;
        }

        let savings = selection_savings(&loaded.graph, &loaded.index.analysis, &selected);
        self.summary.set_label(&format!(
            "{} roots selected · freeable estimate {}",
            selected.len(),
            format::bytes(savings)
        ));

        if self.busy.get() {
            self.detail
                .set_label("measuring: staging roots out, counting dead paths, restoring links");
            self.calculate.set_sensitive(false);
            self.clean.set_sensitive(false);
            return;
        }

        match self.state.last_actual.get() {
            Some(measured) => self.detail.set_label(&format!(
                "measured result: {} dead paths, {} freeable, links restored",
                measured.dead_paths,
                format::bytes(measured.dead_bytes)
            )),
            None => self.detail.set_label(
                "Calculate actual stages the roots out, measures dead paths, and restores the links",
            ),
        }

        self.calculate.set_sensitive(true);
        self.clean.set_sensitive(true);
    }

    fn plan_links(&self) -> (Vec<PlanLink>, Vec<String>, u64) {
        let loaded = match self.state.loaded.borrow().clone() {
            Some(loaded) => loaded,
            None => return (Vec::new(), Vec::new(), 0),
        };
        let selected = self.selected_ids();
        let mut links = Vec::new();
        let mut labels = Vec::new();
        for index in &selected {
            if let Some(root) = loaded.index.roots.get(*index as usize) {
                links.push(PlanLink {
                    link: root.link.clone(),
                    target: root.target.clone(),
                });
                labels.push(root.label.clone());
            }
        }
        let estimate = selection_savings(&loaded.graph, &loaded.index.analysis, &selected);
        (links, labels, estimate)
    }

    fn calculate_actual(self: &Rc<Self>) {
        if self.busy.get() {
            return;
        }
        let (links, _, _) = self.plan_links();
        if links.is_empty() {
            return;
        }
        self.state.last_actual.set(None);
        self.set_busy(true);
        self.update_summary();

        let state = self.state.clone();
        let weak = Rc::downgrade(self);
        let window = self.window.clone();

        clean::spawn(links, "dry-run", move |result| {
            if let Some(panel) = weak.upgrade() {
                panel.set_busy(false);
            }
            match result {
                Ok(output) if output.ok => {
                    state.last_actual.set(Some(Measured {
                        dead_paths: output.dead_paths,
                        dead_bytes: output.dead_bytes,
                    }));
                    if let Some(panel) = weak.upgrade() {
                        panel.update_summary();
                        ui::info(
                            &window,
                            "Measurement finished",
                            &format!(
                                "Staging these roots out makes {} paths dead, freeing {}.\n\nThe links were restored.",
                                output.dead_paths,
                                format::bytes(output.dead_bytes)
                            ),
                        );
                    }
                }
                Ok(output) => {
                    let message = output
                        .error
                        .unwrap_or_else(|| "the helper refused the plan".to_string());
                    if let Some(panel) = weak.upgrade() {
                        panel.update_summary();
                        panel
                            .detail
                            .set_label(&format!("measurement failed: {message}"));
                    }
                    ui::info(&window, "Could not measure", &message);
                }
                Err(error) => {
                    if let Some(panel) = weak.upgrade() {
                        panel.update_summary();
                        panel
                            .detail
                            .set_label(&format!("measurement failed: {error}"));
                    }
                    ui::info(&window, "Could not measure", &error);
                }
            }
        });
    }

    fn confirm_clean(self: &Rc<Self>) {
        let (links, labels, estimate) = self.plan_links();
        if links.is_empty() {
            return;
        }

        let body = format!(
            "This unlinks {} gc roots and runs nix-collect-garbage.\n\nFreeable space: {}\n\nRoots:\n{}",
            links.len(),
            format::bytes(estimate),
            labels
                .iter()
                .map(|label| format!("  · {label}"))
                .collect::<Vec<_>>()
                .join("\n")
        );

        let weak = Rc::downgrade(self);
        let window = self.window.clone();
        let trigger = self.rescan.borrow().clone();
        let dialog_parent = window.clone();

        ui::confirm(
            &dialog_parent,
            "Unlink these gc roots?",
            &body,
            "Unlink and collect",
            move || {
                let weak = weak.clone();
                let window = window.clone();
                let trigger = trigger.clone();
                clean::spawn(links, "clean", move |result| match result {
                    Ok(output) if output.ok => {
                        if let Some(panel) = weak.upgrade() {
                            panel.selected.borrow_mut().clear();
                            panel.state.last_actual.set(None);
                        }
                        ui::info(
                            &window,
                            "Cleaning finished",
                            &format!(
                                "{} paths removed, {} freed.\n\nRefreshing the index.",
                                output.dead_paths,
                                format::bytes(output.dead_bytes)
                            ),
                        );
                        if let Some(trigger) = trigger.as_ref() {
                            trigger();
                        }
                    }
                    Ok(output) => {
                        let message = output
                            .error
                            .unwrap_or_else(|| "the helper refused the plan".to_string());
                        ui::info(&window, "Cleaning failed", &message);
                    }
                    Err(error) => ui::info(&window, "Cleaning failed", &error),
                });
            },
        );
    }
}
