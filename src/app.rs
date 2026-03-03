use crate::capture::PipelineResult;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Search,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputView {
    Stdout,
    Stderr,
}

pub struct App {
    pub result: PipelineResult,
    pub selected_stage: usize,
    pub scroll_offset: usize,
    pub mode: Mode,
    pub output_view: OutputView,
    pub search_query: String,
    pub search_matches: Vec<usize>,
    pub current_match: usize,
    pub should_quit: bool,
}

impl App {
    pub fn new(result: PipelineResult) -> Self {
        Self {
            selected_stage: 0,
            scroll_offset: 0,
            mode: Mode::Normal,
            output_view: OutputView::Stdout,
            search_query: String::new(),
            search_matches: Vec::new(),
            current_match: 0,
            should_quit: false,
            result,
        }
    }

    pub fn stage_count(&self) -> usize {
        self.result.stages.len()
    }

    pub fn current_stage(&self) -> &crate::capture::StageResult {
        &self.result.stages[self.selected_stage]
    }

    pub fn current_output_text(&self) -> String {
        let stage = self.current_stage();
        match self.output_view {
            OutputView::Stdout => String::from_utf8_lossy(&stage.output).to_string(),
            OutputView::Stderr => stage.stderr.clone(),
        }
    }

    pub fn select_next_stage(&mut self) {
        if self.selected_stage < self.stage_count() - 1 {
            self.selected_stage += 1;
            self.scroll_offset = 0;
            self.update_search();
        }
    }

    pub fn select_prev_stage(&mut self) {
        if self.selected_stage > 0 {
            self.selected_stage -= 1;
            self.scroll_offset = 0;
            self.update_search();
        }
    }

    pub fn scroll_down(&mut self, amount: usize) {
        let lines = self.current_output_text().lines().count();
        self.scroll_offset = (self.scroll_offset + amount).min(lines.saturating_sub(1));
    }

    pub fn scroll_up(&mut self, amount: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(amount);
    }

    pub fn toggle_output_view(&mut self) {
        self.output_view = match self.output_view {
            OutputView::Stdout => OutputView::Stderr,
            OutputView::Stderr => OutputView::Stdout,
        };
        self.scroll_offset = 0;
    }

    pub fn start_search(&mut self) {
        self.mode = Mode::Search;
        self.search_query.clear();
        self.search_matches.clear();
        self.current_match = 0;
    }

    pub fn finish_search(&mut self) {
        self.mode = Mode::Normal;
        self.update_search();
        // Jump to first match
        if let Some(&line) = self.search_matches.first() {
            self.scroll_offset = line;
        }
    }

    pub fn cancel_search(&mut self) {
        self.mode = Mode::Normal;
        self.search_query.clear();
        self.search_matches.clear();
    }

    pub fn next_match(&mut self) {
        if !self.search_matches.is_empty() {
            self.current_match = (self.current_match + 1) % self.search_matches.len();
            self.scroll_offset = self.search_matches[self.current_match];
        }
    }

    pub fn prev_match(&mut self) {
        if !self.search_matches.is_empty() {
            self.current_match = if self.current_match == 0 {
                self.search_matches.len() - 1
            } else {
                self.current_match - 1
            };
            self.scroll_offset = self.search_matches[self.current_match];
        }
    }

    fn update_search(&mut self) {
        self.search_matches.clear();
        self.current_match = 0;
        if self.search_query.is_empty() {
            return;
        }
        let query = self.search_query.to_lowercase();
        let text = self.current_output_text();
        for (i, line) in text.lines().enumerate() {
            if line.to_lowercase().contains(&query) {
                self.search_matches.push(i);
            }
        }
    }
}
