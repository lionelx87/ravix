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
        if self.entries.is_empty() {
            self.selected = 0;
            return;
        }
        let last = (self.entries.len() - 1) as isize;
        self.selected = (self.selected as isize + delta).clamp(0, last) as usize;
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

pub fn advance(value: &mut f32, target: f32, step: f32) {
    if *value < target {
        *value = (*value + step).min(target);
    } else if *value > target {
        *value = (*value - step).max(target);
    }
}
