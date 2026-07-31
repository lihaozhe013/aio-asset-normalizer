use crate::modules::i18n::{I18n, LanguagePreference};
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
    SetLanguage(LanguagePreference),
    Quit,
}

pub fn render(
    ui: &mut egui::Ui,
    i18n: &I18n,
    language_preference: LanguagePreference,
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
                ui.menu_button(i18n.tr("menu.file"), |ui| {
                    if ui
                        .button(format!(
                            "{}    Ctrl+O",
                            i18n.tr("menu.import_files")
                        ))
                        .clicked()
                    {
                        actions.push(MenuAction::ImportFiles);
                        ui.close();
                    }
                    if ui
                        .button(format!(
                            "{}   Ctrl+Shift+O",
                            i18n.tr("menu.import_folder")
                        ))
                        .clicked()
                    {
                        actions.push(MenuAction::ImportFolder);
                        ui.close();
                    }
                    ui.separator();
                    if ui.button(i18n.tr("menu.clear_file_list")).clicked() {
                        actions.push(MenuAction::ClearFileList);
                        ui.close();
                    }
                    ui.separator();
                    if ui
                        .button(format!(
                            "{}               Ctrl+Q",
                            i18n.tr("menu.quit")
                        ))
                        .clicked()
                    {
                        actions.push(MenuAction::Quit);
                        ui.close();
                    }
                });

                ui.menu_button(i18n.tr("menu.edit"), |ui| {
                    if ui.button(i18n.tr("menu.preferences")).clicked() {
                        actions.push(MenuAction::OpenPreferences);
                        ui.close();
                    }
                    ui.separator();
                    if ui.button(i18n.tr("menu.reset_defaults")).clicked() {
                        actions.push(MenuAction::ResetConfig);
                        ui.close();
                    }
                });

                ui.menu_button(i18n.tr("menu.view"), |ui| {
                    let check = |b: bool| if b { "[x]" } else { "[ ]" };
                    if ui
                        .button(format!(
                            "{} {}         Ctrl+G",
                            check(show_grid),
                            i18n.tr("menu.show_grid")
                        ))
                        .clicked()
                    {
                        actions.push(MenuAction::ToggleGrid);
                        ui.close();
                    }
                    if ui
                        .button(format!(
                            "{} {}         Ctrl+A",
                            check(show_axes),
                            i18n.tr("menu.show_axes")
                        ))
                        .clicked()
                    {
                        actions.push(MenuAction::ToggleAxes);
                        ui.close();
                    }
                    if ui
                        .button(format!(
                            "{} {}",
                            check(show_origin),
                            i18n.tr("menu.show_origin")
                        ))
                        .clicked()
                    {
                        actions.push(MenuAction::ToggleOrigin);
                        ui.close();
                    }
                    if ui
                        .button(format!(
                            "{} {}        Ctrl+B",
                            check(show_bones),
                            i18n.tr("menu.show_bones")
                        ))
                        .clicked()
                    {
                        actions.push(MenuAction::ToggleBones);
                        ui.close();
                    }
                    ui.separator();
                    if ui
                        .button(format!(
                            "{}      Ctrl+R",
                            i18n.tr("menu.reset_camera")
                        ))
                        .clicked()
                    {
                        actions.push(MenuAction::ResetCamera);
                        ui.close();
                    }
                });

                ui.menu_button(i18n.tr("menu.language"), |ui| {
                    for preference in [
                        LanguagePreference::Auto,
                        LanguagePreference::English,
                        LanguagePreference::Chinese,
                    ] {
                        let selected = language_preference == preference;
                        if ui
                            .selectable_label(selected, preference.label(i18n))
                            .clicked()
                        {
                            actions.push(MenuAction::SetLanguage(preference));
                            ui.close();
                        }
                    }
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
