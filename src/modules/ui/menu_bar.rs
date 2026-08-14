use crate::modules::i18n::I18n;
use three_d::egui;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    GlbEditor,
    BvhStudio,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    ImportGlb,
    ImportBvh,
    ImportMapping,
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
    About,
    SetLanguage(crate::modules::i18n::LanguagePreference),
    Quit,
}

pub fn render(
    ui: &mut egui::Ui,
    i18n: &I18n,
    page: Page,
    show_grid: bool,
    show_axes: bool,
    show_origin: bool,
) -> Vec<MenuAction> {
    let mut actions = Vec::new();
    egui::Frame::NONE
        .inner_margin(egui::Margin::symmetric(6, 2))
        .show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button(i18n.tr("menu.file"), |ui| {
                    if ui.button(i18n.tr("menu.import_glb")).clicked() {
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
                    ui.separator();
                    if ui.button(i18n.tr("menu.save")).clicked() {
                        actions.push(MenuAction::Save);
                        ui.close();
                    }
                    if ui.button(i18n.tr("menu.export")).clicked() {
                        actions.push(MenuAction::Export);
                        ui.close();
                    }
                    ui.separator();
                    if ui.button(i18n.tr("menu.clear_file_list")).clicked() {
                        actions.push(MenuAction::ClearFileList);
                        ui.close();
                    }
                    if ui.button(i18n.tr("menu.quit")).clicked() {
                        actions.push(MenuAction::Quit);
                        ui.close();
                    }
                });

                ui.menu_button(i18n.tr("menu.page"), |ui| {
                    if ui
                        .selectable_label(
                            page == Page::GlbEditor,
                            i18n.tr("page.glb_editor"),
                        )
                        .clicked()
                    {
                        actions.push(MenuAction::OpenGlbEditor);
                        ui.close();
                    }
                    if ui
                        .selectable_label(
                            page == Page::BvhStudio,
                            i18n.tr("page.bvh_studio"),
                        )
                        .clicked()
                    {
                        actions.push(MenuAction::OpenBvhStudio);
                        ui.close();
                    }
                });

                ui.menu_button(i18n.tr("menu.view"), |ui| {
                    if ui
                        .button(format!(
                            "[{}] {}",
                            if show_grid { "x" } else { " " },
                            i18n.tr("menu.show_grid")
                        ))
                        .clicked()
                    {
                        actions.push(MenuAction::ToggleGrid);
                        ui.close();
                    }
                    if ui
                        .button(format!(
                            "[{}] {}",
                            if show_axes { "x" } else { " " },
                            i18n.tr("menu.show_axes")
                        ))
                        .clicked()
                    {
                        actions.push(MenuAction::ToggleAxes);
                        ui.close();
                    }
                    if ui
                        .button(format!(
                            "[{}] {}",
                            if show_origin { "x" } else { " " },
                            i18n.tr("menu.show_origin")
                        ))
                        .clicked()
                    {
                        actions.push(MenuAction::ToggleOrigin);
                        ui.close();
                    }
                    if ui.button(i18n.tr("menu.reset_camera")).clicked() {
                        actions.push(MenuAction::ResetCamera);
                        ui.close();
                    }
                });

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
