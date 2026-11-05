use crate::engine::{WebEngine, servo::ServoEngine};

pub struct Tab {
    pub id: usize,
    pub engine: Box<dyn WebEngine>,
}

impl Tab {
    pub fn new(id: usize, url: &str) -> Self {
        Self {
            id,
            engine: Box::new(ServoEngine::new(url)),
        }
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
}

impl TabManager {
    pub fn new(initial_url: &str) -> Self {
        let first_tab = Tab::new(1, initial_url);
        Self {
            tabs: vec![first_tab],
            active_index: 0,
            next_id: 2,
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

    pub fn resize_all(&mut self, width: u32, height: u32) {
        for tab in &mut self.tabs {
            tab.engine.resize(width, height);
        }
    }

    pub fn new_tab(&mut self, url: &str) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        let old_idx = self.active_index;
        let current_size = if old_idx < self.tabs.len() {
            self.tabs[old_idx].engine.deactivate();
            self.tabs[old_idx].engine.viewport_size()
        } else {
            (1280, 784)
        };
        let mut new_tab = Tab::new(id, url);
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
            if index < self.active_index {
                self.active_index -= 1;
            } else if self.active_index >= self.tabs.len() {
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
    fn test_tab_manager_lifecycle() {
        let mut mgr = TabManager::new("about:neoweb");
        assert_eq!(mgr.len(), 1);
        assert_eq!(mgr.active_tab().title(), "Neoweb - Home");

        // Add a new tab
        let idx = mgr.new_tab("about:blank");
        assert_eq!(idx, 1);
        assert_eq!(mgr.len(), 2);
        assert_eq!(mgr.active_tab().title(), "Blank Page");

        // Navigate between tabs
        mgr.prev_tab();
        assert_eq!(mgr.active_index(), 0);
        assert_eq!(mgr.active_tab().title(), "Neoweb - Home");

        mgr.next_tab();
        assert_eq!(mgr.active_index(), 1);
        assert_eq!(mgr.active_tab().title(), "Blank Page");
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

    #[test]
    fn test_tab_manager_close_earlier_tab_preserves_active_tab_focus() {
        // Create 4 tabs: [Tab0: neoweb, Tab1: blank, Tab2: help, Tab3: blank]
        let mut mgr = TabManager::new("about:neoweb");
        mgr.new_tab("about:blank");
        mgr.new_tab("about:help");
        mgr.new_tab("about:blank");
        assert_eq!(mgr.len(), 4);

        // Focus Tab2 ("about:help")
        mgr.goto_tab(2);
        assert_eq!(mgr.active_index(), 2);
        assert_eq!(mgr.active_tab().title(), "Neoweb Help & Keybindings");

        // Close Tab 0 (an earlier tab)
        assert!(mgr.close_tab(0));
        assert_eq!(mgr.len(), 3);
        // Active index must be adjusted from 2 to 1, still pointing to Tab2 ("about:help")!
        assert_eq!(mgr.active_index(), 1);
        assert_eq!(mgr.active_tab().title(), "Neoweb Help & Keybindings");
    }

    #[test]
    fn test_tab_manager_close_active_tab_shifts_to_valid_tab() {
        // Create 3 tabs, activate the last tab (index 2)
        let mut mgr = TabManager::new("about:neoweb");
        mgr.new_tab("about:blank");
        mgr.new_tab("about:help");
        assert_eq!(mgr.active_index(), 2);
        assert_eq!(mgr.active_tab().title(), "Neoweb Help & Keybindings");

        // Close the active tab (index 2)
        assert!(mgr.close_active_tab());
        assert_eq!(mgr.len(), 2);
        // Should fall back to index 1 ("about:blank")
        assert_eq!(mgr.active_index(), 1);
        assert_eq!(mgr.active_tab().title(), "Blank Page");
    }

    #[test]
    fn test_tab_manager_close_middle_active_tab() {
        let mut mgr = TabManager::new("about:neoweb");
        mgr.new_tab("about:blank");
        mgr.new_tab("about:help");
        mgr.goto_tab(1);
        assert_eq!(mgr.active_index(), 1);
        assert_eq!(mgr.active_tab().title(), "Blank Page");

        // Close middle tab (index 1)
        assert!(mgr.close_tab(1));
        assert_eq!(mgr.len(), 2);
        // Index 1 now holds "about:help"
        assert_eq!(mgr.active_index(), 1);
        assert_eq!(mgr.active_tab().title(), "Neoweb Help & Keybindings");
    }

    #[test]
    fn test_tab_manager_close_later_tab_preserves_active_tab() {
        let mut mgr = TabManager::new("about:neoweb");
        mgr.new_tab("about:blank");
        mgr.new_tab("about:help");
        mgr.goto_tab(0);
        assert_eq!(mgr.active_index(), 0);
        assert_eq!(mgr.active_tab().title(), "Neoweb - Home");

        // Close a later tab (index 2)
        assert!(mgr.close_tab(2));
        assert_eq!(mgr.len(), 2);
        assert_eq!(mgr.active_index(), 0);
        assert_eq!(mgr.active_tab().title(), "Neoweb - Home");
    }
}
