use anyhow::Result;
use crossterm::event::{self, KeyCode, KeyModifiers};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io::Stdout;

use super::types::*;
use super::App;

impl App {
    pub(crate) fn push_jump_location(&mut self) {
        let loc = JumpLocation {
            file_index: self.selected_file,
            line_index: self.selected_line,
            scroll_offset: self.scroll_offset,
        };
        self.jump_stack.push(loc);
        // 上限 100 件
        if self.jump_stack.len() > 100 {
            self.jump_stack.remove(0);
        }
    }

    /// ジャンプスタックから復元
    pub(crate) fn jump_back(&mut self) {
        let Some(loc) = self.jump_stack.pop() else {
            return;
        };

        let file_changed = self.selected_file != loc.file_index;
        self.selected_file = loc.file_index;
        self.selected_line = loc.line_index;
        self.scroll_offset = loc.scroll_offset;

        if file_changed {
            self.update_diff_line_count();
            self.update_file_comment_positions();
            self.ensure_diff_cache();
        }
    }

    /// シンボル選択ポップアップを開く
    pub(crate) async fn open_symbol_popup(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    ) -> Result<()> {
        let file = match self.files().get(self.selected_file) {
            Some(f) => f,
            None => return Ok(()),
        };
        let patch = match file.patch.as_ref() {
            Some(p) => p,
            None => return Ok(()),
        };
        let info = match crate::diff::get_line_info(patch, self.selected_line) {
            Some(i) => i,
            None => return Ok(()),
        };

        let symbols = crate::symbol::extract_all_identifiers(&info.line_content);
        if symbols.is_empty() {
            return Ok(());
        }

        // 候補が1つだけの場合は直接ジャンプ（ポップアップ不要）
        if symbols.len() == 1 {
            let symbol_name = symbols[0].0.clone();
            self.jump_to_symbol_definition_async(&symbol_name, terminal)
                .await?;
            return Ok(());
        }

        self.symbol_popup = Some(SymbolPopupState {
            symbols,
            selected: 0,
        });
        Ok(())
    }

    /// ポップアップ内のキーハンドリング
    pub(crate) async fn handle_symbol_popup_input(
        &mut self,
        key: event::KeyEvent,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    ) -> Result<()> {
        let popup = match self.symbol_popup.as_mut() {
            Some(p) => p,
            None => return Ok(()),
        };

        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                popup.selected = (popup.selected + 1).min(popup.symbols.len().saturating_sub(1));
            }
            KeyCode::Char('k') | KeyCode::Up => {
                popup.selected = popup.selected.saturating_sub(1);
            }
            KeyCode::Enter => {
                let symbol_name = popup.symbols[popup.selected].0.clone();
                self.symbol_popup = None;
                self.jump_to_symbol_definition_async(&symbol_name, terminal)
                    .await?;
            }
            KeyCode::Esc | KeyCode::Char('q') => {
                self.symbol_popup = None;
            }
            _ => {}
        }
        Ok(())
    }

    /// シンボルの定義元へジャンプ（diff パッチ内 → リポジトリ全体、非同期）
    pub(crate) async fn jump_to_symbol_definition_async(
        &mut self,
        symbol: &str,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    ) -> Result<()> {
        // Phase 1: diff パッチ内を検索
        let files: Vec<crate::github::ChangedFile> = self.files().to_vec();
        if let Some((file_idx, line_idx)) =
            crate::symbol::find_definition_in_patches(symbol, &files, self.selected_file)
        {
            self.push_jump_location();
            let file_changed = self.selected_file != file_idx;
            self.selected_file = file_idx;
            self.selected_line = line_idx;
            self.scroll_offset = line_idx;

            if file_changed {
                self.update_diff_line_count();
                self.update_file_comment_positions();
                self.ensure_diff_cache();
            }
            return Ok(());
        }

        // Phase 2: ローカルリポジトリ全体を検索
        let repo_root = match &self.working_dir {
            Some(dir) => {
                let output = tokio::process::Command::new("git")
                    .args(["rev-parse", "--show-toplevel"])
                    .current_dir(dir)
                    .output()
                    .await;
                match output {
                    Ok(o) if o.status.success() => {
                        String::from_utf8_lossy(&o.stdout).trim().to_string()
                    }
                    _ => return Ok(()),
                }
            }
            None => return Ok(()),
        };

        let result =
            crate::symbol::find_definition_in_repo(symbol, std::path::Path::new(&repo_root)).await;
        if let Ok(Some((file_path, line_number))) = result {
            let full_path = std::path::Path::new(&repo_root).join(&file_path);
            let path_str = full_path.to_string_lossy().to_string();

            // ターミナルを一時停止して外部エディタを開く
            crate::ui::restore_terminal(terminal)?;
            let _ = crate::editor::open_file_at_line(
                self.config.editor.as_deref(),
                &path_str,
                line_number,
            );
            *terminal = crate::ui::setup_terminal()?;
        }

        Ok(())
    }

    /// 現在のファイルを外部エディタで開く（gf キー）
    pub(crate) async fn open_current_file_in_editor(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    ) -> Result<()> {
        let file = match self.files().get(self.selected_file) {
            Some(f) => f.clone(),
            None => return Ok(()),
        };

        // 行番号: new_line_number があれば使用、なければ 1
        let line_number = file.patch.as_ref().and_then(|patch| {
            crate::diff::get_line_info(patch, self.selected_line)
                .and_then(|info| info.new_line_number)
        });

        // リポジトリルート取得 → フルパス構築
        let full_path = match &self.working_dir {
            Some(dir) => {
                let output = tokio::process::Command::new("git")
                    .args(["rev-parse", "--show-toplevel"])
                    .current_dir(dir)
                    .output()
                    .await;
                match output {
                    Ok(o) if o.status.success() => {
                        let root = String::from_utf8_lossy(&o.stdout).trim().to_string();
                        std::path::Path::new(&root)
                            .join(&file.filename)
                            .to_string_lossy()
                            .to_string()
                    }
                    _ => return Ok(()),
                }
            }
            None => return Ok(()),
        };

        // TUI 一時停止 → エディタ → TUI 復帰
        crate::ui::restore_terminal(terminal)?;
        let _ = crate::editor::open_file_at_line(
            self.config.editor.as_deref(),
            &full_path,
            line_number.unwrap_or(1) as usize,
        );
        *terminal = crate::ui::setup_terminal()?;

        Ok(())
    }

    pub(crate) fn handle_help_input(
        &mut self,
        key: event::KeyEvent,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    ) -> Result<()> {
        let terminal_height = terminal.size()?.height;
        self.apply_help_scroll(key, terminal_height);
        Ok(())
    }

    /// Help viewport height: tab header (3) + content borders (2) + footer (1)
    pub(crate) const HELP_VIEWPORT_OVERHEAD: u16 = 6;

    pub(crate) fn apply_help_scroll(&mut self, key: event::KeyEvent, terminal_height: u16) {
        let visible_lines = terminal_height.saturating_sub(Self::HELP_VIEWPORT_OVERHEAD) as usize;
        let half_page = (visible_lines / 2).max(1);

        // Read the active tab's scroll offset
        let mut offset = match self.help_tab {
            HelpTab::Keybindings => self.help_scroll_offset,
            HelpTab::Config => self.config_scroll_offset,
        };

        // Check configured close keys first so remapped quit/help keys take priority
        // over bracket tab switching.
        let kb = &self.config.keybindings;
        if self.matches_single_key(&key, &kb.quit)
            || self.matches_single_key(&key, &kb.help)
            || key.code == KeyCode::Esc
        {
            self.state = self.previous_state;
            return;
        }

        // Tab switching ([ / ])
        if matches!(key.code, KeyCode::Char('[') | KeyCode::Char(']')) {
            self.help_tab = match self.help_tab {
                HelpTab::Keybindings => HelpTab::Config,
                HelpTab::Config => HelpTab::Keybindings,
            };
            return;
        }

        if Self::is_shift_char_shortcut(&key, 'j') {
            // Page down (J / Shift+j)
            offset = offset.saturating_add(visible_lines.max(1));
        } else if Self::is_shift_char_shortcut(&key, 'k') {
            // Page up (K / Shift+k)
            offset = offset.saturating_sub(visible_lines.max(1));
        } else if Self::is_shift_char_shortcut(&key, 'g') {
            // Jump to bottom (G / Shift+g)
            offset = usize::MAX;
        } else if matches!(key.code, KeyCode::Char('j') | KeyCode::Down) {
            offset = offset.saturating_add(1);
        } else if matches!(key.code, KeyCode::Char('k') | KeyCode::Up) {
            offset = offset.saturating_sub(1);
        } else if key.code == KeyCode::Char('d') && key.modifiers.contains(KeyModifiers::CONTROL) {
            offset = offset.saturating_add(half_page);
        } else if key.code == KeyCode::Char('u') && key.modifiers.contains(KeyModifiers::CONTROL) {
            offset = offset.saturating_sub(half_page);
        } else if key.code == KeyCode::Char('g') && key.modifiers.is_empty() {
            // Jump to top (g without modifiers)
            offset = 0;
        }

        // Write back to the active tab's scroll offset
        match self.help_tab {
            HelpTab::Keybindings => self.help_scroll_offset = offset,
            HelpTab::Config => self.config_scroll_offset = offset,
        };
    }
}
