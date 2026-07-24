use three_d::egui;

pub enum MenuAction {
    ImportFiles,
    ImportFolder,
    ClearFileList,
    ResetConfig,
    ResetCamera,
    ToggleGrid,
    ToggleAxes,
    ToggleOrigin,
    ToggleBones,
    OpenPreferences,
    About,
    Quit,
}

pub fn render(
    ui: &mut egui::Ui,
    show_grid: bool,
    show_axes: bool,
    show_origin: bool,
    show_bones: bool,
) -> Vec<MenuAction> {
    let mut actions = Vec::new();

    egui::Frame::NONE
        .inner_margin(egui::Margin::symmetric(6, 2))
        .show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Import Files...    Ctrl+O").clicked() {
                        actions.push(MenuAction::ImportFiles);
                        ui.close();
                    }
                    if ui.button("Import Folder...   Ctrl+Shift+O").clicked() {
                        actions.push(MenuAction::ImportFolder);
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Clear File List").clicked() {
                        actions.push(MenuAction::ClearFileList);
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Quit               Ctrl+Q").clicked() {
                        actions.push(MenuAction::Quit);
                        ui.close();
                    }
                });

                ui.menu_button("Edit", |ui| {
                    if ui.button("Preferences...").clicked() {
                        actions.push(MenuAction::OpenPreferences);
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Reset All to Defaults").clicked() {
                        actions.push(MenuAction::ResetConfig);
                        ui.close();
                    }
                });

                ui.menu_button("View", |ui| {
                    let check = |b: bool| if b { "[x]" } else { "[ ]" };
                    if ui
                        .button(format!(
                            "{} Show Grid         Ctrl+G",
                            check(show_grid)
                        ))
                        .clicked()
                    {
                        actions.push(MenuAction::ToggleGrid);
                        ui.close();
                    }
                    if ui
                        .button(format!(
                            "{} Show Axes         Ctrl+A",
                            check(show_axes)
                        ))
                        .clicked()
                    {
                        actions.push(MenuAction::ToggleAxes);
                        ui.close();
                    }
                    if ui
                        .button(format!("{} Show Origin", check(show_origin)))
                        .clicked()
                    {
                        actions.push(MenuAction::ToggleOrigin);
                        ui.close();
                    }
                    if ui
                        .button(format!(
                            "{} Show Bones        Ctrl+B",
                            check(show_bones)
                        ))
                        .clicked()
                    {
                        actions.push(MenuAction::ToggleBones);
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Reset Camera      Ctrl+R").clicked() {
                        actions.push(MenuAction::ResetCamera);
                        ui.close();
                    }
                });

                ui.menu_button("Help", |ui| {
                    if ui.button("About AIO Asset Normalizer").clicked() {
                        actions.push(MenuAction::About);
                        ui.close();
                    }
                });
            });
        });

    actions
}
