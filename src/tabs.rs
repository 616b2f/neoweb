use crate::engine::{WebEngine, servo::ServoEngine};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EngineKind {
    #[default]
    Servo,
}

pub struct Tab {
    pub id: usize,
    pub engine: Box<dyn WebEngine>,
    pub kind: EngineKind,
}

impl Tab {
    pub fn new(id: usize, url: &str) -> Self {
        Self::new_with_engine(id, url, EngineKind::default())
    }

    pub fn new_with_engine(id: usize, url: &str, kind: EngineKind) -> Self {
        let engine: Box<dyn WebEngine> = match kind {
            EngineKind::Servo => Box::new(ServoEngine::new(url)),
        };
        Self { id, engine, kind }
    }

    pub fn title(&self) -> &str {
        self.engine.title()
    }

    pub fn uri(&self) -> &str {
        self.engine.uri()
    }

    pub fn favicon(&self) -> Option<&crate::engine::Favicon> {
        self.engine.favicon()
    }

    pub fn is_loading(&self) -> bool {
        self.engine.status() != crate::engine::LoadStatus::Finished
    }
}

pub struct TabManager {
    tabs: Vec<Tab>,
    active_index: usize,
    next_id: usize,
    default_engine: EngineKind,
}

impl TabManager {
    pub fn new(initial_url: &str) -> Self {
        Self::new_with_engine(initial_url, EngineKind::default())
    }

    pub fn new_with_engine(initial_url: &str, default_engine: EngineKind) -> Self {
        let first_tab = Tab::new_with_engine(1, initial_url, default_engine);
        Self {
            tabs: vec![first_tab],
            active_index: 0,
            next_id: 2,
            default_engine,
        }
    }

    pub fn tabs(&self) -> &[Tab] {
        &self.tabs
    }

    pub fn active_index(&self) -> usize {
        self.active_index
    }

    pub fn active_tab(&self) -> &Tab {
        &self.tabs[self.active_index]
    }

    pub fn active_tab_mut(&mut self) -> &mut Tab {
        &mut self.tabs[self.active_index]
    }

    pub fn active_engine(&self) -> &dyn WebEngine {
        self.tabs[self.active_index].engine.as_ref()
    }

    pub fn active_engine_mut(&mut self) -> &mut dyn WebEngine {
        self.tabs[self.active_index].engine.as_mut()
    }

    pub fn default_engine(&self) -> EngineKind {
        self.default_engine
    }

    pub fn set_default_engine(&mut self, kind: EngineKind) {
        self.default_engine = kind;
    }

    pub fn active_engine_kind(&self) -> EngineKind {
        self.tabs[self.active_index].kind
    }

    pub fn switch_active_engine(&mut self, kind: EngineKind) {
        let id = self.tabs[self.active_index].id;
        let uri = self.tabs[self.active_index].uri().to_string();
        self.tabs[self.active_index].engine.deactivate();
        let mut new_tab = Tab::new_with_engine(id, &uri, kind);
        new_tab.engine.activate();
        self.tabs[self.active_index] = new_tab;
    }

    pub fn resize_all(&mut self, width: u32, height: u32) {
        for tab in &mut self.tabs {
            tab.engine.resize(width, height);
        }
    }

    pub fn new_tab(&mut self, url: &str) -> usize {
        self.new_tab_with_engine(url, self.default_engine)
    }

    pub fn new_tab_with_engine(&mut self, url: &str, kind: EngineKind) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        let old_idx = self.active_index;
        let current_size = if old_idx < self.tabs.len() {
            self.tabs[old_idx].engine.deactivate();
            self.tabs[old_idx].engine.viewport_size()
        } else {
            (1280, 784)
        };
        let mut new_tab = Tab::new_with_engine(id, url, kind);
        new_tab.engine.resize(current_size.0, current_size.1);
        new_tab.engine.activate();
        self.tabs.push(new_tab);
        self.active_index = self.tabs.len() - 1;
        self.active_index
    }

    pub fn close_tab(&mut self, index: usize) -> bool {
        if self.tabs.len() <= 1 {
            // Keep at least one tab or signal quit
            return false;
        }

        if index < self.tabs.len() {
            self.tabs[index].engine.deactivate();
            self.tabs.remove(index);
            if self.active_index >= self.tabs.len() {
                self.active_index = self.tabs.len() - 1;
            }
            self.tabs[self.active_index].engine.activate();
            true
        } else {
            false
        }
    }

    pub fn close_active_tab(&mut self) -> bool {
        self.close_tab(self.active_index)
    }

    /// Deactivates and cleanly closes all tabs.
    pub fn close_all(&mut self) {
        for tab in &mut self.tabs {
            tab.engine.deactivate();
        }
        self.tabs.clear();
        self.active_index = 0;
    }

    pub fn next_tab(&mut self) {
        if !self.tabs.is_empty() {
            let old_idx = self.active_index;
            self.active_index = (self.active_index + 1) % self.tabs.len();
            if old_idx != self.active_index {
                self.tabs[old_idx].engine.deactivate();
                self.tabs[self.active_index].engine.activate();
            }
        }
    }

    pub fn prev_tab(&mut self) {
        if !self.tabs.is_empty() {
            let old_idx = self.active_index;
            if self.active_index == 0 {
                self.active_index = self.tabs.len() - 1;
            } else {
                self.active_index -= 1;
            }
            if old_idx != self.active_index {
                self.tabs[old_idx].engine.deactivate();
                self.tabs[self.active_index].engine.activate();
            }
        }
    }

    pub fn goto_tab(&mut self, index: usize) {
        if index < self.tabs.len() && index != self.active_index {
            self.tabs[self.active_index].engine.deactivate();
            self.active_index = index;
            self.tabs[self.active_index].engine.activate();
        }
    }

    pub fn len(&self) -> usize {
        self.tabs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tab_manager_with_engine_kind() {
        let mut mgr = TabManager::new_with_engine("about:neoweb", EngineKind::Servo);
        assert_eq!(mgr.len(), 1);
        assert_eq!(mgr.active_engine_kind(), EngineKind::Servo);
        assert_eq!(mgr.active_tab().title(), "Neoweb - Home");

        // Add a new tab
        let idx = mgr.new_tab_with_engine("about:blank", EngineKind::Servo);
        assert_eq!(idx, 1);
        assert_eq!(mgr.len(), 2);
        assert_eq!(mgr.active_engine_kind(), EngineKind::Servo);
        assert_eq!(mgr.active_tab().title(), "Blank Page");

        // Switch active tab engine
        mgr.switch_active_engine(EngineKind::Servo);
        assert_eq!(mgr.active_engine_kind(), EngineKind::Servo);
        assert_eq!(mgr.active_tab().title(), "Blank Page");
    }

    #[test]
    fn test_engine_kind_default() {
        assert_eq!(EngineKind::default(), EngineKind::Servo);
        let mgr = TabManager::new("about:neoweb");
        assert_eq!(mgr.active_engine_kind(), EngineKind::Servo);
    }

    #[test]
    fn test_tab_manager_close_all() {
        let mut mgr = TabManager::new("about:neoweb");
        mgr.new_tab("about:blank");
        assert_eq!(mgr.len(), 2);
        assert!(!mgr.is_empty());

        mgr.close_all();
        assert_eq!(mgr.len(), 0);
        assert!(mgr.is_empty());
        assert_eq!(mgr.active_index(), 0);
    }
}
