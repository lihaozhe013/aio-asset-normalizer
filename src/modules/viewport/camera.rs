use three_d::*;

pub struct OrbitCamera {
    pub camera: Camera,
    target: Vec3,
    radius: f32,
    theta: f32,
    phi: f32,
    min_radius: f32,
    max_radius: f32,
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
            0.1,
            100.0,
        );

        Self {
            camera,
            target,
            radius,
            theta,
            phi,
            min_radius: 0.5,
            max_radius: 50.0,
            rotating: false,
            panning: false,
        }
    }

    pub fn set_viewport(&mut self, viewport: Viewport) {
        if viewport.width < 1 || viewport.height < 1 {
            return;
        }
        self.camera.set_viewport(viewport);
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
        let position = vec3(4.0, 3.0, 6.0);
        self.target = vec3(0.0, 0.5, 0.0);
        let direction = position - self.target;
        self.radius = direction.magnitude();
        self.theta = f32::atan2(direction.z, direction.x);
        self.phi = f32::acos(direction.y / self.radius);
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
            0.1,
            100.0,
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
}
