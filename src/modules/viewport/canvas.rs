use super::helpers;
use three_d::*;

pub struct ViewportCanvas {
    pub axes: Gm<Mesh, ColorMaterial>,
    pub grid: Gm<Mesh, ColorMaterial>,
    pub origin_sphere: Gm<Mesh, ColorMaterial>,
}

impl ViewportCanvas {
    pub fn new(context: &Context) -> Self {
        Self {
            axes: helpers::build_axes(context),
            grid: helpers::build_grid(context),
            origin_sphere: helpers::build_origin_sphere(context),
        }
    }
}
