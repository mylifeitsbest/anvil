//! Анимации: кривые, параметры из настроек и хранилище твинов.
//!
//! Перерисовка запрашивается только пока что-то реально движется —
//! в покое Anvil не тратит ни одного кадра.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};

use anvil_config::{Anim, AnimationSettings, Easing, Motion};

pub fn ease(e: Easing, t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    match e {
        Easing::Linear => t,
        Easing::OutCubic => 1.0 - (1.0 - t).powi(3),
        Easing::OutQuint => 1.0 - (1.0 - t).powi(5),
        Easing::InOutCubic => {
            if t < 0.5 {
                4.0 * t * t * t
            } else {
                1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
            }
        }
        Easing::OutBack => {
            let c1 = 1.701_58;
            let c3 = c1 + 1.0;
            1.0 + c3 * (t - 1.0).powi(3) + c1 * (t - 1.0).powi(2)
        }
        Easing::Spring => {
            if t >= 1.0 {
                1.0
            } else {
                1.0 - (-6.5 * t).exp() * (t * 10.0).cos()
            }
        }
    }
}

/// Готовые параметры одной анимации с учётом главного выключателя и скорости.
#[derive(Clone, Copy, Debug)]
pub struct Spec {
    pub dur: f32,
    pub easing: Easing,
    pub style: Motion,
}

impl Spec {
    pub fn of(s: &AnimationSettings, a: &Anim) -> Self {
        let on = s.enabled && a.enabled && a.style != Motion::None && a.duration_ms > 0;
        Self { dur: if on { a.duration_ms as f32 / 1000.0 / s.speed.max(0.05) } else { 0.0 }, easing: a.easing.unwrap_or(s.easing), style: a.style }
    }

    pub fn instant() -> Self {
        Self { dur: 0.0, easing: Easing::Linear, style: Motion::None }
    }
}

pub fn key(k: impl Hash) -> u64 {
    let mut h = DefaultHasher::new();
    k.hash(&mut h);
    h.finish()
}

#[derive(Clone, Copy)]
struct Tween {
    from: f32,
    to: f32,
    start: f64,
    dur: f32,
    easing: Easing,
    touched: u64,
}

impl Tween {
    fn at(&self, now: f64) -> (f32, bool) {
        if self.dur <= 0.0 {
            return (self.to, true);
        }
        let k = ((now - self.start) as f32 / self.dur).clamp(0.0, 1.0);
        (self.from + (self.to - self.from) * ease(self.easing, k), k >= 1.0)
    }
}

/// Хранилище твинов по ключу: значение плавно догоняет цель.
#[derive(Default)]
pub struct Tweens {
    map: HashMap<u64, Tween>,
    pub now: f64,
    /// Что-то ещё движется — нужен следующий кадр.
    pub active: bool,
    frame: u64,
}

impl Tweens {
    pub fn begin_frame(&mut self, now: f64) {
        self.now = now;
        self.active = false;
        self.frame += 1;
        if self.frame.is_multiple_of(900) {
            let f = self.frame;
            self.map.retain(|_, t| f - t.touched < 900);
        }
    }

    /// Текущее значение для цели `target`. Смена цели запускает новый твин из текущей точки.
    pub fn value(&mut self, k: impl Hash, target: f32, spec: Spec) -> f32 {
        let id = key(k);
        let now = self.now;
        let frame = self.frame;
        let t = self.map.entry(id).or_insert(Tween { from: target, to: target, start: now, dur: spec.dur, easing: spec.easing, touched: frame });
        t.touched = frame;
        if (t.to - target).abs() > 1e-4 {
            let (cur, _) = t.at(now);
            *t = Tween { from: cur, to: target, start: now, dur: spec.dur, easing: spec.easing, touched: frame };
        }
        let (v, done) = t.at(now);
        if !done {
            self.active = true;
        }
        v
    }

    /// Как `value`, но стартует из `from` при первом обращении (появление элемента).
    pub fn enter(&mut self, k: impl Hash, from: f32, target: f32, spec: Spec) -> f32 {
        let id = key(k);
        let now = self.now;
        let frame = self.frame;
        self.map.entry(id).or_insert(Tween { from, to: target, start: now, dur: spec.dur, easing: spec.easing, touched: frame });
        self.value_by_id(id, target, spec)
    }

    fn value_by_id(&mut self, id: u64, target: f32, spec: Spec) -> f32 {
        let now = self.now;
        let frame = self.frame;
        let Some(t) = self.map.get_mut(&id) else { return target };
        t.touched = frame;
        if (t.to - target).abs() > 1e-4 {
            let (cur, _) = t.at(now);
            *t = Tween { from: cur, to: target, start: now, dur: spec.dur, easing: spec.easing, touched: frame };
        }
        let (v, done) = t.at(now);
        if !done {
            self.active = true;
        }
        v
    }

    /// Прогресс 0..1 от момента `start`.
    pub fn since(&mut self, start: f64, spec: Spec) -> f32 {
        if spec.dur <= 0.0 {
            return 1.0;
        }
        let k = ((self.now - start) as f32 / spec.dur).clamp(0.0, 1.0);
        if k < 1.0 {
            self.active = true;
        }
        ease(spec.easing, k)
    }
}

/// Экспоненциальное сглаживание к цели (для прокрутки).
pub fn smooth_towards(current: f32, target: f32, dt: f32, tau: f32) -> f32 {
    if tau <= 0.0 || (target - current).abs() < 0.35 {
        return target;
    }
    current + (target - current) * (1.0 - (-dt / tau).exp())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn easings_hit_endpoints() {
        for e in [Easing::Linear, Easing::OutCubic, Easing::OutQuint, Easing::InOutCubic, Easing::OutBack, Easing::Spring] {
            assert!(ease(e, 0.0).abs() < 1e-3, "{e:?}");
            assert!((ease(e, 1.0) - 1.0).abs() < 1e-3, "{e:?}");
        }
    }

    #[test]
    fn tween_retargets_from_current() {
        let mut t = Tweens::default();
        let spec = Spec { dur: 1.0, easing: Easing::Linear, style: Motion::Fade };
        t.begin_frame(0.0);
        assert_eq!(t.value("x", 0.0, spec), 0.0);
        t.begin_frame(0.0);
        t.value("x", 10.0, spec);
        t.begin_frame(0.5);
        let mid = t.value("x", 10.0, spec);
        assert!((mid - 5.0).abs() < 1e-3);
        assert!(t.active);
        t.begin_frame(2.0);
        assert_eq!(t.value("x", 10.0, spec), 10.0);
        assert!(!t.active);
    }
}
