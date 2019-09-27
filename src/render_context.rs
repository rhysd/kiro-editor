#[derive(PartialEq, Clone, Copy, Debug)]
pub enum DrawMessage {
    Open,
    Close,
    Update,
    DoNothing,
}

impl Default for DrawMessage {
    fn default() -> Self {
        DrawMessage::DoNothing
    }
}

#[derive(Default)]
pub struct RenderContext {
    dirty_start: Option<usize>,
    cursor_moved: bool,
    draw_message: DrawMessage,
    update_highlight: bool,
}

impl RenderContext {
    pub fn rerender() -> Self {
        let mut c = Self::default();
        c.dirty_start = Some(0);
        c.update_highlight = true;
        c
    }

    pub fn set_dirty_start(&mut self, line: usize) {
        if let Some(l) = self.dirty_start {
            if l <= line {
                return;
            }
        }
        self.dirty_start = Some(line);
    }

    pub fn dirty_start(&self) -> Option<usize> {
        self.dirty_start
    }
}
