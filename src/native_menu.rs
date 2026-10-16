//! Main-thread adapter between the macOS application menu and application actions.

use crate::{
    app::App,
    modules::{
        build_info::APP_NAME,
        i18n::LanguagePreference,
        ui::menu_bar::{MenuAction, Page},
    },
};
use muda::{
    accelerator::{Accelerator, Code, Modifiers},
    CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu,
};

pub(crate) struct NativeMenu {
    // Retain the menu tree for the lifetime of the event loop.
    _menu: Menu,
    submenus: Vec<(Submenu, &'static str)>,
    items: Vec<(MenuItem, &'static str, MenuAction)>,
    checks: Vec<(CheckMenuItem, &'static str, MenuAction)>,
    language: LanguagePreference,
    page: Option<Page>,
}

impl NativeMenu {
    pub(crate) fn new(app: &App) -> muda::Result<Self> {
        let menu = Menu::new();
        let application = Submenu::new(APP_NAME, true);
        let mut native = Self {
            _menu: menu,
            submenus: Vec::new(),
            items: Vec::new(),
            checks: Vec::new(),
            language: app.i18n.preference(),
            page: None,
        };
        native._menu.append(&application)?;
        native.add_item(&application, "menu.about", MenuAction::About, None, app)?;
        application.append_items(&[
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::services(None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::hide(Some(&format!("Hide {APP_NAME}"))),
            &PredefinedMenuItem::hide_others(None),
            &PredefinedMenuItem::show_all(None),
            &PredefinedMenuItem::separator(),
        ])?;
        native.add_item(
            &application,
            "menu.quit",
            MenuAction::Quit,
            Some(Code::KeyQ),
            app,
        )?;

        let file = native.add_submenu("menu.file", app)?;
        for (key, action, shortcut) in [
            ("menu.import_glb", MenuAction::ImportGlb, None),
            ("menu.import_bvh", MenuAction::ImportBvh, None),
            ("menu.import_mapping", MenuAction::ImportMapping, None),
            ("menu.export_mapping", MenuAction::ExportMapping, None),
        ] {
            native.add_item(&file, key, action, shortcut, app)?;
        }
        file.append(&PredefinedMenuItem::separator())?;
        native.add_item(&file, "menu.save", MenuAction::Save, Some(Code::KeyS), app)?;
        native.add_item(
            &file,
            "menu.export",
            MenuAction::Export,
            Some(Code::KeyE),
            app,
        )?;
        file.append(&PredefinedMenuItem::separator())?;
        native.add_item(
            &file,
            "menu.clear_file_list",
            MenuAction::ClearFileList,
            None,
            app,
        )?;

        let view = native.add_submenu("menu.view", app)?;
        for (key, action) in [
            ("menu.show_grid", MenuAction::ToggleGrid),
            ("menu.show_axes", MenuAction::ToggleAxes),
            ("menu.show_origin", MenuAction::ToggleOrigin),
        ] {
            let item = CheckMenuItem::new(app.i18n.tr(key), true, false, None);
            view.append(&item)?;
            native.checks.push((item, key, action));
        }
        view.append(&PredefinedMenuItem::separator())?;
        native.add_item(
            &view,
            "menu.reset_camera",
            MenuAction::ResetCamera,
            None,
            app,
        )?;

        let language = native.add_submenu("menu.language", app)?;
        for preference in [
            LanguagePreference::Auto,
            LanguagePreference::English,
            LanguagePreference::Chinese,
        ] {
            let item = CheckMenuItem::new(preference.label(&app.i18n), true, false, None);
            language.append(&item)?;
            native
                .checks
                .push((item, "", MenuAction::SetLanguage(preference)));
        }
        let help = native.add_submenu("menu.help", app)?;
        native.add_item(&help, "Connect AI", MenuAction::ConnectAi, None, app)?;
        native.sync(app)?;
        native._menu.init_for_nsapp();
        help.set_as_help_menu_for_nsapp();
        Ok(native)
    }

    fn add_submenu(&mut self, key: &'static str, app: &App) -> muda::Result<Submenu> {
        let submenu = Submenu::new(app.i18n.tr(key), true);
        self._menu.append(&submenu)?;
        self.submenus.push((submenu.clone(), key));
        Ok(submenu)
    }

    fn add_item(
        &mut self,
        parent: &Submenu,
        key: &'static str,
        action: MenuAction,
        shortcut: Option<Code>,
        app: &App,
    ) -> muda::Result<()> {
        let label = match action {
            MenuAction::Quit => format!("{} {APP_NAME}", app.i18n.tr(key)),
            _ => app.i18n.tr(key).to_owned(),
        };
        let item = MenuItem::new(label, true, shortcut.map(command));
        parent.append(&item)?;
        self.items.push((item, key, action));
        Ok(())
    }

    pub(crate) fn dispatch_pending(&self, app: &mut App) {
        // The render loop continuously polls, so menu events are handled on the
        // next frame without installing a process-global event handler.
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            let action = self
                .items
                .iter()
                .find_map(|(item, _, action)| (item.id() == &event.id).then_some(*action))
                .or_else(|| {
                    self.checks
                        .iter()
                        .find_map(|(item, _, action)| (item.id() == &event.id).then_some(*action))
                });
            if let Some(action) = action {
                app.dispatch_action(&action);
            }
        }
    }

    pub(crate) fn sync(&mut self, app: &App) -> muda::Result<()> {
        if self.language != app.i18n.preference() {
            self.language = app.i18n.preference();
            for (submenu, key) in &self.submenus {
                submenu.set_text(app.i18n.tr(key));
            }
            for (item, key, action) in &self.items {
                let label = match action {
                    MenuAction::Quit => {
                        format!("{} {APP_NAME}", app.i18n.tr(key))
                    }
                    _ => app.i18n.tr(key).to_owned(),
                };
                item.set_text(label);
            }
            for (item, key, action) in &self.checks {
                item.set_text(match action {
                    MenuAction::SetLanguage(preference) => preference.label(&app.i18n),
                    _ => app.i18n.tr(key),
                });
            }
        }
        for (item, _, action) in &self.checks {
            let checked = match action {
                MenuAction::ToggleGrid => app.canvas.show_grid,
                MenuAction::ToggleAxes => app.canvas.show_axes,
                MenuAction::ToggleOrigin => app.canvas.show_origin,
                MenuAction::SetLanguage(preference) => *preference == app.i18n.preference(),
                _ => false,
            };
            if item.is_checked() != checked {
                item.set_checked(checked);
            }
        }
        if self.page != Some(app.page) {
            // Clear both accelerators first, then assign Cmd+O to the active workflow.
            for (item, _, action) in &self.items {
                if matches!(action, MenuAction::ImportGlb | MenuAction::ImportBvh) {
                    item.set_accelerator(None)?;
                }
            }
            let import = if app.page == Page::BvhStudio {
                MenuAction::ImportBvh
            } else {
                MenuAction::ImportGlb
            };
            for (item, _, action) in &self.items {
                if *action == import {
                    item.set_accelerator(Some(command(Code::KeyO)))?;
                }
            }
            self.page = Some(app.page);
        }
        Ok(())
    }
}

fn command(key: Code) -> Accelerator {
    Accelerator::new(Modifiers::META, key)
}
