use three_d::*;

pub struct OrbitCamera {
    pub camera: Camera,
    target: Vec3,
    radius: f32,
    theta: f32,
    phi: f32,
    min_radius: f32,
    max_radius: f32,
    near_plane: f32,
    far_plane: f32,
    focused_bounds: Option<([f32; 3], [f32; 3])>,
    rotating: bool,
    panning: bool,
}

impl OrbitCamera {
    pub fn new(viewport: Viewport) -> Self {
        let position = vec3(4.0, 3.0, 6.0);
        let target = vec3(0.0, 0.5, 0.0);
        let direction = position - target;
        let radius = direction.magnitude();
        let theta = f32::atan2(direction.z, direction.x);
        let phi = f32::acos(direction.y / radius);

        let camera = Camera::new_perspective(
            viewport,
            position,
            target,
            vec3(0.0, 1.0, 0.0),
            degrees(45.0),
            0.01,
            100.0,
        );

        Self {
            camera,
            target,
            radius,
            theta,
            phi,
            min_radius: 0.05,
            max_radius: 50.0,
            near_plane: 0.01,
            far_plane: 100.0,
            focused_bounds: None,
            rotating: false,
            panning: false,
        }
    }

    pub fn set_viewport(&mut self, viewport: Viewport) {
        if viewport.width < 1 || viewport.height < 1 {
            return;
        }
        let changed = self.camera.viewport().width != viewport.width
            || self.camera.viewport().height != viewport.height;
        self.camera.set_viewport(viewport);
        if changed {
            if let Some((minimum, maximum)) = self.focused_bounds {
                self.focus_on_bounds(minimum, maximum);
            }
        }
    }

    pub fn handle_events(&mut self, events: &[Event], viewport: Viewport) {
        for event in events {
            match event {
                Event::MousePress {
                    button,
                    position,
                    handled,
                    ..
                } if !handled
                    && pointer_inside_viewport(*position, viewport) =>
                {
                    match button {
                        MouseButton::Left => {
                            self.rotating = true;
                        }
                        MouseButton::Middle => {
                            self.panning = true;
                        }
                        _ => {}
                    }
                }
                Event::MouseRelease { button, .. } => match button {
                    MouseButton::Left => self.rotating = false,
                    MouseButton::Middle => self.panning = false,
                    _ => {}
                },
                Event::MouseMotion {
                    delta,
                    position,
                    handled,
                    ..
                } if !handled
                    && pointer_inside_viewport(*position, viewport) =>
                {
                    if self.rotating {
                        let sensitivity = 0.005;
                        self.theta += delta.0 * sensitivity;
                        self.phi -= delta.1 * sensitivity;
                        self.phi =
                            self.phi.clamp(0.05, std::f32::consts::PI - 0.05);
                        self.update_camera_view();
                    }
                    if self.panning {
                        let sensitivity = self.radius * 0.001;
                        let forward =
                            (self.target - self.camera_position()).normalize();
                        let right = forward.cross(vec3(0.0, 1.0, 0.0));
                        let up = right.cross(forward);
                        self.target -= right * (delta.0 * sensitivity);
                        self.target += up * (delta.1 * sensitivity);
                        self.update_camera_view();
                    }
                }
                Event::MouseWheel {
                    delta,
                    position,
                    handled,
                    ..
                } if !handled
                    && pointer_inside_viewport(*position, viewport) =>
                {
                    // three-d scales mouse-wheel deltas by LINE_HEIGHT (24.0)
                    // before delivering them, so delta.1 is approximately
                    // +/-24 per physical wheel notch (and proportionally for
                    // smooth/trackpad scrolling). Normalize to "notches" so
                    // the sensitivity constant has an intuitive meaning.
                    //
                    // Then apply exponential zoom: each notch scales the
                    // orbit radius by a fixed ratio, so the same scroll
                    // distance produces the same proportional change
                    // regardless of zoom level -- fine control when zoomed
                    // in, quick traversal when zoomed out.
                    let notches = delta.1 / 24.0;
                    let zoom_per_notch: f32 = 0.95;
                    self.radius *= zoom_per_notch.powf(notches);
                    self.radius =
                        self.radius.clamp(self.min_radius, self.max_radius);
                    self.update_camera_view();
                }
                _ => {}
            }
        }
    }

    pub fn reset(&mut self) {
        self.focused_bounds = None;
        self.min_radius = 0.05;
        self.max_radius = 50.0;
        self.near_plane = 0.01;
        self.far_plane = 100.0;
        let position = vec3(4.0, 3.0, 6.0);
        self.target = vec3(0.0, 0.5, 0.0);
        let direction = position - self.target;
        self.radius = direction.magnitude();
        self.theta = f32::atan2(direction.z, direction.x);
        self.phi = f32::acos(direction.y / self.radius);
        self.update_camera_view();
    }

    /// Center and frame a set of world-space points.
    ///
    /// BVH files often contain absolute root translations and may use units
    /// that make the skeleton much smaller or larger than the editor's
    /// default scene. Framing the converted points keeps the first preview
    /// useful without changing the authored coordinates or retargeting math.
    pub fn focus_on_points(&mut self, points: &[[f32; 3]]) {
        let mut minimum = vec3(f32::INFINITY, f32::INFINITY, f32::INFINITY);
        let mut maximum =
            vec3(f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);
        let mut has_point = false;
        for point in points {
            if point.iter().any(|value| !value.is_finite()) {
                continue;
            }
            let value = vec3(point[0], point[1], point[2]);
            minimum.x = minimum.x.min(value.x);
            minimum.y = minimum.y.min(value.y);
            minimum.z = minimum.z.min(value.z);
            maximum.x = maximum.x.max(value.x);
            maximum.y = maximum.y.max(value.y);
            maximum.z = maximum.z.max(value.z);
            has_point = true;
        }
        if !has_point {
            return;
        }

        self.focus_on_bounds(
            [minimum.x, minimum.y, minimum.z],
            [maximum.x, maximum.y, maximum.z],
        );
    }

    /// Center and frame an axis-aligned world-space bounding box.
    ///
    /// The camera distance is derived from the vertical and horizontal field
    /// of view, while clipping planes and orbit limits follow the same scale.
    /// This keeps millimetre, metre and large mechanical skeletons equally
    /// usable without changing their authored units.
    pub fn focus_on_bounds(&mut self, minimum: [f32; 3], maximum: [f32; 3]) {
        if minimum
            .iter()
            .chain(maximum.iter())
            .any(|value| !value.is_finite())
        {
            return;
        }
        let minimum = vec3(minimum[0], minimum[1], minimum[2]);
        let maximum = vec3(maximum[0], maximum[1], maximum[2]);
        let diagonal = (maximum - minimum).magnitude().max(1.0e-7);
        let sphere_radius = (diagonal * 0.5).max(1.0e-7);
        self.target = (minimum + maximum) * 0.5;
        let viewport = self.camera.viewport();
        let aspect =
            (viewport.width.max(1) as f32) / (viewport.height.max(1) as f32);
        let vertical_half_fov = 45.0_f32.to_radians() * 0.5;
        let horizontal_half_fov = (vertical_half_fov.tan() * aspect).atan();
        let half_fov = vertical_half_fov.min(horizontal_half_fov).max(0.05);
        let distance = (sphere_radius / half_fov.sin()) * 1.12;
        self.min_radius = (sphere_radius * 0.002).max(1.0e-7);
        self.max_radius = (sphere_radius * 2_000.0).max(distance * 20.0);
        self.radius = distance.clamp(self.min_radius, self.max_radius);
        self.near_plane = (sphere_radius * 0.01).max(self.radius * 0.0001);
        self.far_plane = (self.radius + sphere_radius * 2.5)
            .max(self.near_plane * 100.0)
            .max(sphere_radius * 10.0)
            .max(1.0e-5);
        self.focused_bounds = Some((
            [minimum.x, minimum.y, minimum.z],
            [maximum.x, maximum.y, maximum.z],
        ));
        self.update_camera_view();
    }

    fn camera_position(&self) -> Vec3 {
        let x = self.radius * self.phi.sin() * self.theta.cos();
        let y = self.radius * self.phi.cos();
        let z = self.radius * self.phi.sin() * self.theta.sin();
        vec3(x, y, z) + self.target
    }

    fn update_camera_view(&mut self) {
        let position = self.camera_position();
        self.camera = Camera::new_perspective(
            self.camera.viewport(),
            position,
            self.target,
            vec3(0.0, 1.0, 0.0),
            degrees(45.0),
            self.near_plane,
            self.far_plane,
        );
    }
}

fn pointer_inside_viewport(
    position: PhysicalPoint,
    viewport: Viewport,
) -> bool {
    let left = viewport.x as f32;
    let bottom = viewport.y as f32;
    let right = left + viewport.width as f32;
    let top = bottom + viewport.height as f32;
    position.x >= left
        && position.x < right
        && position.y >= bottom
        && position.y < top
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mouse_press(button: MouseButton) -> Event {
        mouse_press_at(button, (0.0, 0.0))
    }

    fn mouse_press_at(button: MouseButton, position: (f32, f32)) -> Event {
        Event::MousePress {
            button,
            position: position.into(),
            modifiers: Modifiers::default(),
            handled: false,
        }
    }

    fn mouse_release(button: MouseButton, handled: bool) -> Event {
        Event::MouseRelease {
            button,
            position: (0.0, 0.0).into(),
            modifiers: Modifiers::default(),
            handled,
        }
    }

    fn canvas_viewport() -> Viewport {
        Viewport::new_at_origo(640, 480)
    }

    #[test]
    fn left_mouse_button_controls_rotation() {
        let mut camera = OrbitCamera::new(Viewport::new_at_origo(640, 480));

        camera.handle_events(
            &[mouse_press(MouseButton::Right)],
            canvas_viewport(),
        );
        assert!(!camera.rotating);

        camera.handle_events(
            &[mouse_press(MouseButton::Left)],
            canvas_viewport(),
        );
        assert!(camera.rotating);

        camera.handle_events(
            &[mouse_release(MouseButton::Left, false)],
            canvas_viewport(),
        );
        assert!(!camera.rotating);
    }

    #[test]
    fn handled_pointer_events_do_not_control_camera() {
        let mut camera = OrbitCamera::new(Viewport::new_at_origo(640, 480));
        let initial_theta = camera.theta;

        camera.handle_events(
            &[Event::MousePress {
                button: MouseButton::Left,
                position: (0.0, 0.0).into(),
                modifiers: Modifiers::default(),
                handled: true,
            }],
            canvas_viewport(),
        );
        assert!(!camera.rotating);

        camera.handle_events(
            &[Event::MouseMotion {
                button: Some(MouseButton::Left),
                delta: (10.0, 0.0),
                position: (10.0, 0.0).into(),
                modifiers: Modifiers::default(),
                handled: true,
            }],
            canvas_viewport(),
        );
        assert_eq!(camera.theta, initial_theta);

        camera.handle_events(
            &[mouse_press(MouseButton::Left)],
            canvas_viewport(),
        );
        assert!(camera.rotating);
        camera.handle_events(
            &[mouse_release(MouseButton::Left, true)],
            canvas_viewport(),
        );
        assert!(!camera.rotating);
    }

    #[test]
    fn pointer_events_outside_canvas_do_not_control_camera() {
        let mut camera = OrbitCamera::new(Viewport::new_at_origo(640, 480));
        let canvas = Viewport {
            x: 200,
            y: 100,
            width: 400,
            height: 300,
        };
        let initial_theta = camera.theta;
        let initial_radius = camera.radius;

        camera.handle_events(
            &[mouse_press_at(MouseButton::Left, (100.0, 200.0))],
            canvas,
        );
        camera.handle_events(
            &[Event::MouseWheel {
                delta: (0.0, 24.0),
                position: (100.0, 200.0).into(),
                modifiers: Modifiers::default(),
                handled: false,
            }],
            canvas,
        );
        assert!(!camera.rotating);
        assert_eq!(camera.theta, initial_theta);
        assert_eq!(camera.radius, initial_radius);

        camera.handle_events(
            &[mouse_press_at(MouseButton::Left, (300.0, 200.0))],
            canvas,
        );
        assert!(camera.rotating);
        camera.handle_events(
            &[Event::MouseMotion {
                button: Some(MouseButton::Left),
                delta: (10.0, 0.0),
                position: (100.0, 200.0).into(),
                modifiers: Modifiers::default(),
                handled: false,
            }],
            canvas,
        );
        assert_eq!(camera.theta, initial_theta);
        camera
            .handle_events(&[mouse_release(MouseButton::Left, false)], canvas);
        assert!(!camera.rotating);
    }

    #[test]
    fn horizontal_drag_updates_orbit_direction() {
        let mut camera = OrbitCamera::new(Viewport::new_at_origo(640, 480));
        let initial_theta = camera.theta;

        camera.handle_events(
            &[
                mouse_press(MouseButton::Left),
                Event::MouseMotion {
                    button: Some(MouseButton::Left),
                    delta: (10.0, 0.0),
                    position: (10.0, 0.0).into(),
                    modifiers: Modifiers::default(),
                    handled: false,
                },
            ],
            canvas_viewport(),
        );

        assert!(camera.theta > initial_theta);
    }

    #[test]
    fn focus_on_points_centers_and_scales_the_orbit() {
        let mut camera = OrbitCamera::new(Viewport::new_at_origo(640, 480));
        camera.focus_on_points(&[[-2.0, 1.0, 0.0], [2.0, 5.0, 0.0]]);

        assert_eq!(camera.target, vec3(0.0, 3.0, 0.0));
        assert!(camera.radius < 9.0);
        assert!(camera.radius > camera.min_radius);
    }

    #[test]
    fn focus_handles_tiny_and_large_skeletons_with_scaled_clipping() {
        let mut camera = OrbitCamera::new(Viewport::new_at_origo(640, 480));
        camera.focus_on_points(&[[0.0, 0.0, 0.0], [0.0, 0.004, 0.0]]);
        let tiny_radius = camera.radius;
        let tiny_near = camera.near_plane;
        let tiny_far = camera.far_plane;
        assert!(tiny_radius < 0.1);
        assert!(tiny_near > 0.0);
        assert!(tiny_far > tiny_radius);

        camera.focus_on_points(&[[0.0, 0.0, 0.0], [0.0, 100.0, 0.0]]);
        assert!(camera.radius > tiny_radius * 1_000.0);
        assert!(camera.far_plane > tiny_far * 1_000.0);
        assert!(camera.near_plane > tiny_near * 1_000.0);
    }
}
