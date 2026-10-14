use crate::ao3::models::Work;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchMatch {
    pub chapter_index: usize, // 1-based
    pub paragraph_index: usize,
    pub match_text: String,
}

#[derive(Debug, Default, Clone)]
pub struct ReaderSearchState {
    pub is_open: bool,
    pub query: String,
    pub matches: Vec<SearchMatch>,
    pub current_match_idx: usize,
    pub search_all_chapters: bool,
}

impl ReaderSearchState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn execute_search(&mut self, work: &Work, current_chapter_idx: usize) {
        self.matches.clear();
        self.current_match_idx = 0;

        let q = self.query.trim().to_lowercase();
        if q.is_empty() {
            return;
        }

        for chapter in &work.chapters {
            if !self.search_all_chapters && chapter.index != current_chapter_idx {
                continue;
            }

            for (p_idx, para) in chapter.paragraphs.iter().enumerate() {
                if para.to_lowercase().contains(&q) {
                    self.matches.push(SearchMatch {
                        chapter_index: chapter.index,
                        paragraph_index: p_idx,
                        match_text: para.clone(),
                    });
                }
            }
        }
    }

    pub fn next_match(&mut self) -> Option<&SearchMatch> {
        if self.matches.is_empty() {
            return None;
        }
        self.current_match_idx = (self.current_match_idx + 1) % self.matches.len();
        self.matches.get(self.current_match_idx)
    }

    pub fn prev_match(&mut self) -> Option<&SearchMatch> {
        if self.matches.is_empty() {
            return None;
        }
        if self.current_match_idx == 0 {
            self.current_match_idx = self.matches.len() - 1;
        } else {
            self.current_match_idx -= 1;
        }
        self.matches.get(self.current_match_idx)
    }

    pub fn current_match(&self) -> Option<&SearchMatch> {
        self.matches.get(self.current_match_idx)
    }
}
