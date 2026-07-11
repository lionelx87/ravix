pub struct SlidePanel<T> {
    pub entries: Vec<T>,
    pub selected: usize,
    pub slide: f32,
    pub target: f32,
}

impl<T> SlidePanel<T> {
    pub fn opening(entries: Vec<T>) -> Self {
        Self {
            entries,
            selected: 0,
            slide: 0.0,
            target: 1.0,
        }
    }

    pub fn focused(&self) -> Option<&T> {
        self.entries.get(self.selected)
    }

    pub fn move_selection(&mut self, delta: isize) {
        self.selected = clamp_index(self.selected, delta, self.entries.len());
    }

    pub fn refresh(&mut self, entries: Vec<T>) {
        self.selected = self.selected.min(entries.len().saturating_sub(1));
        self.entries = entries;
    }

    pub fn close(&mut self) {
        self.target = 0.0;
    }

    pub fn is_sliding(&self) -> bool {
        self.slide != self.target
    }

    pub fn advance(&mut self, step: f32) {
        advance(&mut self.slide, self.target, step);
    }

    pub fn is_dismissed(&self) -> bool {
        self.target == 0.0 && self.slide <= 0.0
    }
}

pub fn clamp_index(current: usize, delta: isize, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    let last = (len - 1) as isize;
    (current as isize + delta).clamp(0, last) as usize
}

pub fn advance_panel<T>(panel: &mut Option<SlidePanel<T>>, step: f32) -> bool {
    let dismissed = panel.as_mut().is_some_and(|slide| {
        slide.advance(step);
        slide.is_dismissed()
    });
    if dismissed {
        *panel = None;
    }
    dismissed
}

pub fn is_animating<T>(panel: &Option<SlidePanel<T>>) -> bool {
    panel.as_ref().is_some_and(SlidePanel::is_sliding)
}

pub fn advance(value: &mut f32, target: f32, step: f32) {
    if *value < target {
        *value = (*value + step).min(target);
    } else if *value > target {
        *value = (*value - step).max(target);
    }
}
