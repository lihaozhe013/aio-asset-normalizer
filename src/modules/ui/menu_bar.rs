#[cfg(not(target_os = "macos"))]
use crate::modules::i18n::I18n;
#[cfg(not(target_os = "macos"))]
use three_d::egui;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    GlbEditor,
    BvhStudio,
    FbxConverter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    ImportGlb,
    ImportBvh,
    ImportMapping,
    ExportMapping,
    Save,
    Export,
    ExportBvhGlb,
    ExportBvhAnimationClip,
    ClearFileList,
    ResetCamera,
    ToggleGrid,
    ToggleAxes,
    ToggleOrigin,
    OpenGlbEditor,
    OpenBvhStudio,
    OpenFbxConverter,
    ConnectAi,
    About,
    SetLanguage(crate::modules::i18n::LanguagePreference),
    Quit,
}

#[cfg(not(target_os = "macos"))]
pub fn render(
    ui: &mut egui::Ui,
    i18n: &I18n,
    show_grid: bool,
    show_axes: bool,
    show_origin: bool,
) -> Vec<MenuAction> {
    let mut actions = Vec::new();
    egui::Frame::NONE
        .inner_margin(egui::Margin::symmetric(0, 2))
        .show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button(i18n.tr("menu.file"), |ui| {
                    if ui
                        .button(i18n.tr("menu.import_glb"))
                        .on_hover_text(shortcut_label())
                        .clicked()
                    {
                        actions.push(MenuAction::ImportGlb);
                        ui.close();
                    }
                    if ui.button(i18n.tr("menu.import_bvh")).clicked() {
                        actions.push(MenuAction::ImportBvh);
                        ui.close();
                    }
                    if ui.button(i18n.tr("menu.import_mapping")).clicked() {
                        actions.push(MenuAction::ImportMapping);
                        ui.close();
                    }
                    if ui.button(i18n.tr("menu.export_mapping")).clicked() {
                        actions.push(MenuAction::ExportMapping);
                        ui.close();
                    }
                    ui.separator();
                    if ui
                        .button(i18n.tr("menu.save"))
                        .on_hover_text(shortcut_label())
                        .clicked()
                    {
                        actions.push(MenuAction::Save);
                        ui.close();
                    }
                    if ui
                        .button(i18n.tr("menu.export"))
                        .on_hover_text(shortcut_label())
                        .clicked()
                    {
                        actions.push(MenuAction::Export);
                        ui.close();
                    }
                    ui.separator();
                    if ui.button(i18n.tr("menu.clear_file_list")).clicked() {
                        actions.push(MenuAction::ClearFileList);
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Connect AI").clicked() {
                        actions.push(MenuAction::ConnectAi);
                        ui.close();
                    }
                    if ui.button(i18n.tr("menu.quit")).clicked() {
                        actions.push(MenuAction::Quit);
                        ui.close();
                    }
                });

                ui.menu_button(i18n.tr("menu.view"), |ui| {
                    let mut grid = show_grid;
                    if ui.checkbox(&mut grid, i18n.tr("menu.show_grid")).changed() {
                        actions.push(MenuAction::ToggleGrid);
                        ui.close();
                    }
                    let mut axes = show_axes;
                    if ui.checkbox(&mut axes, i18n.tr("menu.show_axes")).changed() {
                        actions.push(MenuAction::ToggleAxes);
                        ui.close();
                    }
                    let mut origin = show_origin;
                    if ui
                        .checkbox(&mut origin, i18n.tr("menu.show_origin"))
                        .changed()
                    {
                        actions.push(MenuAction::ToggleOrigin);
                        ui.close();
                    }
                    ui.separator();
                    if ui.button(i18n.tr("menu.reset_camera")).clicked() {
                        actions.push(MenuAction::ResetCamera);
                        ui.close();
                    }
                    ui.separator();
                    ui.menu_button(i18n.tr("menu.language"), |ui| {
                        for preference in [
                            crate::modules::i18n::LanguagePreference::Auto,
                            crate::modules::i18n::LanguagePreference::English,
                            crate::modules::i18n::LanguagePreference::Chinese,
                        ] {
                            if ui
                                .selectable_label(
                                    i18n.preference() == preference,
                                    preference.label(i18n),
                                )
                                .clicked()
                            {
                                actions.push(MenuAction::SetLanguage(preference));
                                ui.close();
                            }
                        }
                    });
                });

                ui.menu_button(i18n.tr("menu.help"), |ui| {
                    if ui.button(i18n.tr("menu.about")).clicked() {
                        actions.push(MenuAction::About);
                        ui.close();
                    }
                });
            });
        });
    actions
}

/// Human-readable text for the global import/save/export shortcut.
///
/// The command modifier is Cmd on macOS and Ctrl on other platforms, so the
/// label mentions both.
#[cfg(not(target_os = "macos"))]
fn shortcut_label() -> String {
    "Ctrl/Cmd+O / S / E".to_owned()
}
