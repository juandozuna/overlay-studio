//! Entrance, score-bump and winner-reveal keyframe presets (WN-6).
use crate::model::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Preset {
    Entrance,
    ScoreBump,
    LeaderPulse,
    WinnerReveal,
}

fn frame(element: &Element, time: f32, scale: f32, opacity: f32, dy: f32, easing: Easing) -> Keyframe {
    let mut k = Keyframe::capture(element, time);
    k.w = (element.w * scale).clamp(1., 16384.);
    k.h = (element.h * scale).clamp(1., 16384.);
    k.x = element.x + (element.w - k.w) / 2.;
    k.y = element.y + (element.h - k.h) / 2. + dy;
    k.opacity = opacity.clamp(0., 1.);
    k.easing = easing;
    k
}

pub fn entrance_stagger(element: &Element, index: usize, total: usize) -> Vec<Keyframe> {
    let delay = 0.08 * index.min(total.saturating_sub(1)).min(1000) as f32;
    let mut frames = vec![frame(element, 0., 1., 0., 40., Easing::Hold)];
    if delay > 0. {
        frames.push(frame(element, delay, 1., 0., 40., Easing::EaseOut));
    }
    frames.push(frame(element, delay + 0.15, 1., element.opacity, 40., Easing::EaseOutBack));
    frames.push(frame(element, delay + 0.65, 1., element.opacity, 0., Easing::EaseOut));
    frames
}

pub fn score_bump(element: &Element) -> Vec<Keyframe> {
    vec![
        frame(element, 0., 1., element.opacity, 0., Easing::EaseOut),
        frame(element, 0.15, 1.35, element.opacity, 0., Easing::Spring),
        frame(element, 0.6, 1., element.opacity, 0., Easing::EaseOut),
    ]
}

pub fn leader_pulse(element: &Element) -> Vec<Keyframe> {
    let dim = element.opacity * 0.75;
    vec![
        frame(element, 0., 1., element.opacity, 0., Easing::EaseInOut),
        frame(element, 0.6, 1.05, dim, 0., Easing::EaseInOut),
        frame(element, 1.2, 1., element.opacity, 0., Easing::EaseInOut),
    ]
}

pub fn winner_reveal(element: &Element) -> Vec<Keyframe> {
    vec![
        frame(element, 0., 0.6, 0., 0., Easing::EaseOut),
        frame(element, 0.2, 0.6, element.opacity, 0., Easing::EaseOutBack),
        frame(element, 0.8, 1., element.opacity, 0., Easing::EaseOut),
    ]
}

pub fn apply_preset(element: &mut Element, preset: Preset, index: usize, total: usize) {
    element.keyframes = match preset {
        Preset::Entrance => entrance_stagger(element, index, total),
        Preset::ScoreBump => score_bump(element),
        Preset::LeaderPulse => leader_pulse(element),
        Preset::WinnerReveal => winner_reveal(element),
    };
    element.loop_animation = preset == Preset::LeaderPulse;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all(e: &Element) -> Vec<(Preset, Vec<Keyframe>)> {
        vec![
            (Preset::Entrance, entrance_stagger(e, 0, 5)),
            (Preset::Entrance, entrance_stagger(e, 4, 5)),
            (Preset::ScoreBump, score_bump(e)),
            (Preset::LeaderPulse, leader_pulse(e)),
            (Preset::WinnerReveal, winner_reveal(e)),
        ]
    }

    #[test]
    fn presets_are_sorted_and_valid() {
        let mut e = Element::new(1, Kind::Rectangle);
        e.w = 120.;
        e.h = 40.;
        for (_, frames) in all(&e) {
            assert!(frames.windows(2).all(|p| p[0].time <= p[1].time));
            let mut el = e.clone();
            el.keyframes = frames;
            let d = Design {
                id: 1,
                name: "t".into(),
                width: 1920,
                height: 1080,
                elements: vec![el],
            };
            assert!(validate_design(&d).is_ok());
        }
    }

    #[test]
    fn apply_preset_sets_loop_only_for_pulse() {
        let mut e = Element::new(1, Kind::Rectangle);
        apply_preset(&mut e, Preset::LeaderPulse, 0, 1);
        assert!(e.loop_animation);
        apply_preset(&mut e, Preset::ScoreBump, 0, 1);
        assert!(!e.loop_animation);
        apply_preset(&mut e, Preset::Entrance, 3, 5);
        assert!((e.keyframes[1].time - 0.24).abs() < 1e-6);
        assert_eq!(e.keyframes[0].opacity, 0.);
        assert_eq!(e.keyframes[0].y, e.y + 40.);
    }
}
