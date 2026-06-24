use std::time::Instant;
use kore_css::{TimingFunction, Transform, TransformValue, Transition};

#[derive(Debug, Clone)]
pub enum AnimatableValue {
    Float(f32),
    Color(f32, f32, f32, f32),
    Transform(Transform),
}

#[derive(Debug, Clone)]
pub struct AnimationState {
    pub node_id: usize,
    pub property: String,
    pub from_value: AnimatableValue,
    pub to_value: AnimatableValue,
    pub duration_ms: f32,
    pub elapsed_ms: f32,
    pub timing: TimingFunction,
    pub started_at: Instant,
    pub finished: bool,
}

impl AnimationState {
    pub fn progress(&self) -> f32 {
        if self.duration_ms <= 0.0 { return 1.0; }
        (self.elapsed_ms / self.duration_ms).clamp(0.0, 1.0)
    }

    pub fn eased_progress(&self) -> f32 {
        let t = self.progress();
        match self.timing {
            TimingFunction::Linear => t,
            TimingFunction::Ease => cubic_bezier(t, 0.25, 0.1, 0.25, 1.0),
            TimingFunction::EaseIn => cubic_bezier(t, 0.42, 0.0, 1.0, 1.0),
            TimingFunction::EaseOut => cubic_bezier(t, 0.0, 0.0, 0.58, 1.0),
            TimingFunction::EaseInOut => cubic_bezier(t, 0.42, 0.0, 0.58, 1.0),
        }
    }

    pub fn current_opacity(&self) -> Option<f32> {
        if self.property != "opacity" { return None; }
        if let (AnimatableValue::Float(from), AnimatableValue::Float(to)) =
            (&self.from_value, &self.to_value)
        {
            let t = self.eased_progress();
            Some(from + (to - from) * t)
        } else {
            None
        }
    }

    pub fn current_transform_offset(&self) -> Option<(f32, f32)> {
        if self.property != "transform" { return None; }
        if let (AnimatableValue::Transform(from), AnimatableValue::Transform(to)) =
            (&self.from_value, &self.to_value)
        {
            let t = self.eased_progress();
            let from_tx = extract_translate(from);
            let to_tx = extract_translate(to);
            Some((
                from_tx.0 + (to_tx.0 - from_tx.0) * t,
                from_tx.1 + (to_tx.1 - from_tx.1) * t,
            ))
        } else {
            None
        }
    }
}

fn extract_translate(transform: &Transform) -> (f32, f32) {
    for tv in transform {
        match tv {
            TransformValue::Translate(x, y) => return (*x, *y),
            TransformValue::TranslateX(x) => return (*x, 0.0),
            TransformValue::TranslateY(y) => return (0.0, *y),
            _ => {}
        }
    }
    (0.0, 0.0)
}

fn cubic_bezier(t: f32, x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    let mut x = t;
    for _ in 0..8 {
        let bx = bezier_coord(x, x1, x2) - t;
        let dx = bezier_deriv(x, x1, x2);
        if dx.abs() < 1e-6 { break; }
        x -= bx / dx;
    }
    bezier_coord(x, y1, y2)
}

fn bezier_coord(t: f32, p1: f32, p2: f32) -> f32 {
    3.0 * t * (1.0 - t) * (1.0 - t) * p1
        + 3.0 * t * t * (1.0 - t) * p2
        + t * t * t
}

fn bezier_deriv(t: f32, p1: f32, p2: f32) -> f32 {
    3.0 * (1.0 - t) * (1.0 - t) * p1
        + 6.0 * t * (1.0 - t) * (p2 - p1)
        + 3.0 * t * t * (1.0 - p2)
}

pub struct AnimationEngine {
    pub active: Vec<AnimationState>,
}

impl AnimationEngine {
    pub fn new() -> Self {
        Self { active: Vec::new() }
    }

    pub fn tick(&mut self) {
        let now = Instant::now();
        for anim in &mut self.active {
            if !anim.finished {
                anim.elapsed_ms = now.duration_since(anim.started_at).as_millis() as f32;
                if anim.elapsed_ms >= anim.duration_ms {
                    anim.elapsed_ms = anim.duration_ms;
                    anim.finished = true;
                }
            }
        }
        self.active.retain(|a| !a.finished);
    }

    pub fn has_active(&self) -> bool {
        !self.active.is_empty()
    }

    pub fn start_transition(
        &mut self,
        node_id: usize,
        property: &str,
        from: AnimatableValue,
        to: AnimatableValue,
        transition: &Transition,
    ) {
        self.active.retain(|a| !(a.node_id == node_id && a.property == property));
        self.active.push(AnimationState {
            node_id,
            property: property.to_string(),
            from_value: from,
            to_value: to,
            duration_ms: transition.duration_ms,
            elapsed_ms: 0.0,
            timing: transition.timing.clone(),
            started_at: Instant::now(),
            finished: false,
        });
    }

    pub fn get_opacity(&self, node_id: usize) -> Option<f32> {
        self.active.iter()
            .find(|a| a.node_id == node_id && a.property == "opacity")
            .and_then(|a| a.current_opacity())
    }

    pub fn get_translate(&self, node_id: usize) -> Option<(f32, f32)> {
        self.active.iter()
            .find(|a| a.node_id == node_id && a.property == "transform")
            .and_then(|a| a.current_transform_offset())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn animation_engine_tick_advances_progress() {
        let mut engine = AnimationEngine::new();
        engine.start_transition(
            1,
            "opacity",
            AnimatableValue::Float(0.0),
            AnimatableValue::Float(1.0),
            &Transition {
                property: "opacity".to_string(),
                duration_ms: 1000.0,
                timing: TimingFunction::Linear,
                delay_ms: 0.0,
            },
        );
        assert!(engine.has_active());
        engine.tick();
        assert!(engine.has_active());
    }

    #[test]
    fn cubic_bezier_linear_is_identity() {
        let mut anim = AnimationState {
            node_id: 0,
            property: "opacity".to_string(),
            from_value: AnimatableValue::Float(0.0),
            to_value: AnimatableValue::Float(1.0),
            duration_ms: 1000.0,
            elapsed_ms: 500.0,
            timing: TimingFunction::Linear,
            started_at: Instant::now(),
            finished: false,
        };
        let opacity = anim.current_opacity().unwrap();
        assert!((opacity - 0.5).abs() < 0.05);
    }

    #[test]
    fn opacity_transition_interpolates() {
        let mut engine = AnimationEngine::new();
        engine.start_transition(
            1,
            "opacity",
            AnimatableValue::Float(0.0),
            AnimatableValue::Float(1.0),
            &Transition {
                property: "opacity".to_string(),
                duration_ms: 100.0,
                timing: TimingFunction::Linear,
                delay_ms: 0.0,
            },
        );
        // Simulate time advance by manually setting elapsed
        if let Some(anim) = engine.active.first_mut() {
            anim.elapsed_ms = 50.0;
        }
        let opacity = engine.get_opacity(1).unwrap();
        assert!((opacity - 0.5).abs() < 0.05);
    }

    #[test]
    fn translate_transition_interpolates() {
        let mut engine = AnimationEngine::new();
        engine.start_transition(
            1,
            "transform",
            AnimatableValue::Transform(vec![TransformValue::TranslateX(0.0)]),
            AnimatableValue::Transform(vec![TransformValue::TranslateX(100.0)]),
            &Transition {
                property: "transform".to_string(),
                duration_ms: 200.0,
                timing: TimingFunction::Linear,
                delay_ms: 0.0,
            },
        );
        if let Some(anim) = engine.active.first_mut() {
            anim.elapsed_ms = 100.0;
        }
        let (tx, ty) = engine.get_translate(1).unwrap();
        assert!((tx - 50.0).abs() < 1.0);
        assert!((ty - 0.0).abs() < 0.01);
    }
}
