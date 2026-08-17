#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GlbReloadKind {
    OpenModel,
    EditedModel,
}

pub(crate) fn merge_glb_reload_kind(
    current: Option<GlbReloadKind>,
    requested: GlbReloadKind,
) -> GlbReloadKind {
    match (current, requested) {
        (Some(GlbReloadKind::OpenModel), _) | (_, GlbReloadKind::OpenModel) => {
            GlbReloadKind::OpenModel
        }
        _ => GlbReloadKind::EditedModel,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opening_a_model_takes_precedence_over_pending_edit_reload() {
        assert_eq!(
            merge_glb_reload_kind(
                Some(GlbReloadKind::EditedModel),
                GlbReloadKind::OpenModel,
            ),
            GlbReloadKind::OpenModel
        );
        assert_eq!(
            merge_glb_reload_kind(
                Some(GlbReloadKind::OpenModel),
                GlbReloadKind::EditedModel,
            ),
            GlbReloadKind::OpenModel
        );
    }

    #[test]
    fn edit_reloads_can_be_coalesced_without_resetting_camera() {
        assert_eq!(
            merge_glb_reload_kind(None, GlbReloadKind::EditedModel),
            GlbReloadKind::EditedModel
        );
        assert_eq!(
            merge_glb_reload_kind(
                Some(GlbReloadKind::EditedModel),
                GlbReloadKind::EditedModel,
            ),
            GlbReloadKind::EditedModel
        );
    }
}
