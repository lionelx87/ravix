use crate::palette::fuzzy_filter;

pub struct BranchPicker<Context> {
    pub context: Context,
    pub query: String,
    names: Vec<String>,
    matches: Vec<usize>,
    pub selected: usize,
}

impl<Context> BranchPicker<Context> {
    pub fn opening(context: Context, names: Vec<String>) -> Self {
        Self {
            context,
            query: String::new(),
            matches: (0..names.len()).collect(),
            names,
            selected: 0,
        }
    }

    pub fn input(&mut self, character: char) {
        self.query.push(character);
        self.refilter();
    }

    pub fn backspace(&mut self) {
        self.query.pop();
        self.refilter();
    }

    fn refilter(&mut self) {
        let names: Vec<&str> = self.names.iter().map(String::as_str).collect();
        self.matches = fuzzy_filter(&self.query, &names);
        self.selected = 0;
    }

    pub fn move_selection(&mut self, delta: isize) {
        self.selected = crate::slide::clamp_index(self.selected, delta, self.matches.len());
    }

    pub fn rows(&self) -> Vec<&str> {
        self.matches
            .iter()
            .map(|&index| self.names[index].as_str())
            .collect()
    }

    pub fn chosen(&self) -> Option<&str> {
        let index = *self.matches.get(self.selected)?;
        Some(self.names[index].as_str())
    }
}
