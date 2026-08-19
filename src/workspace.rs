//! 開いているプロジェクトの集合と切り替えの管理。
//!
//! プロジェクトはブラウザのタブグループのように**複数を並列に開いておき**、
//! タブエリアには現在のプロジェクトのシートだけを表示する。
//! デフォルトプロジェクトは常に開いており、閉じることができない。
//!
//! 状態遷移はインデックス演算だけで完結するため、純粋なロジックとして
//! 切り出して単体テストの対象にしている。

use crate::project::{ProjectStore, DEFAULT_PROJECT_ID};

/// 開いているプロジェクトの集合と、現在アクティブなプロジェクト。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workspace {
    /// 開いている順（先頭は必ずデフォルトプロジェクト）
    open: Vec<String>,
    /// 現在表示しているプロジェクト
    active: String,
}

impl Default for Workspace {
    fn default() -> Self {
        Self {
            open: vec![DEFAULT_PROJECT_ID.to_string()],
            active: DEFAULT_PROJECT_ID.to_string(),
        }
    }
}

impl Workspace {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn open_ids(&self) -> &[String] {
        &self.open
    }

    pub fn active(&self) -> &str {
        &self.active
    }

    pub fn is_open(&self, id: &str) -> bool {
        self.open.iter().any(|x| x == id)
    }

    /// 保存されていた状態から復元する。
    ///
    /// * デフォルトプロジェクトは必ず先頭に入る
    /// * 既に削除されたプロジェクトは取り除く
    /// * アクティブが開いていない場合はデフォルトに戻す
    pub fn restore(open: &[String], active: Option<&str>, store: &ProjectStore) -> Self {
        let mut ids = vec![DEFAULT_PROJECT_ID.to_string()];
        for id in open {
            if id == DEFAULT_PROJECT_ID {
                continue;
            }
            if store.find(id).is_some() && !ids.iter().any(|x| x == id) {
                ids.push(id.clone());
            }
        }
        let active = active
            .filter(|a| ids.iter().any(|x| x == a))
            .unwrap_or(DEFAULT_PROJECT_ID)
            .to_string();
        Self { open: ids, active }
    }

    /// プロジェクトを開いてアクティブにする。
    /// 既に開いていれば重複させず、アクティブを切り替えるだけ。
    pub fn open_project(&mut self, id: &str) {
        if !self.is_open(id) {
            self.open.push(id.to_string());
        }
        self.active = id.to_string();
    }

    /// 開いているプロジェクトへ切り替える。開いていない ID は無視する。
    pub fn switch(&mut self, id: &str) -> bool {
        if !self.is_open(id) {
            return false;
        }
        self.active = id.to_string();
        true
    }

    /// プロジェクトを閉じる。閉じた場合のみ true。
    ///
    /// デフォルトプロジェクトは閉じられない。
    /// アクティブなものを閉じた場合は、その左隣（無ければ右隣）へ移る。
    pub fn close(&mut self, id: &str) -> bool {
        if id == DEFAULT_PROJECT_ID || !self.is_open(id) {
            return false;
        }
        let pos = match self.open.iter().position(|x| x == id) {
            Some(p) => p,
            None => return false,
        };
        self.open.remove(pos);
        if self.active == id {
            let next = if pos > 0 { pos - 1 } else { 0 };
            self.active = self
                .open
                .get(next)
                .cloned()
                .unwrap_or_else(|| DEFAULT_PROJECT_ID.to_string());
        }
        true
    }

    /// そのシートを表示すべきプロジェクトを返す。
    ///
    /// 同じシートが複数のプロジェクトに所属していても画面には 1 箇所にしか
    /// 出さないため、**開いている順で最初に所属しているもの**を所有者とする。
    pub fn owner_of<'a>(&'a self, sheet_keys: &[String], store: &ProjectStore) -> Option<&'a str> {
        self.open
            .iter()
            .find(|pid| sheet_keys.iter().any(|k| store.contains_sheet(pid, k)))
            .map(|s| s.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: u64 = 1_750_000_000_000;

    fn store_with(ids: &[(&str, &str)]) -> ProjectStore {
        let mut store = ProjectStore::new();
        store.ensure_default(T0);
        for (id, name) in ids {
            store.add(name, "", id, T0).unwrap();
        }
        store
    }

    #[test]
    fn new_workspace_has_only_the_default_project() {
        let ws = Workspace::new();
        assert_eq!(ws.open_ids(), &[DEFAULT_PROJECT_ID.to_string()]);
        assert_eq!(ws.active(), DEFAULT_PROJECT_ID);
    }

    #[test]
    fn opening_a_project_appends_and_activates_it() {
        let mut ws = Workspace::new();
        ws.open_project("id-1");

        assert_eq!(ws.open_ids(), &[DEFAULT_PROJECT_ID.to_string(), "id-1".to_string()]);
        assert_eq!(ws.active(), "id-1");
    }

    #[test]
    fn opening_an_already_open_project_only_switches() {
        let mut ws = Workspace::new();
        ws.open_project("id-1");
        ws.open_project("id-2");
        ws.open_project("id-1");

        // 重複して並ばないこと
        assert_eq!(
            ws.open_ids(),
            &[DEFAULT_PROJECT_ID.to_string(), "id-1".to_string(), "id-2".to_string()]
        );
        assert_eq!(ws.active(), "id-1");
    }

    #[test]
    fn switching_requires_the_project_to_be_open() {
        let mut ws = Workspace::new();
        ws.open_project("id-1");

        assert!(ws.switch(DEFAULT_PROJECT_ID));
        assert_eq!(ws.active(), DEFAULT_PROJECT_ID);
        // 開いていないものへは切り替わらない
        assert!(!ws.switch("id-unknown"));
        assert_eq!(ws.active(), DEFAULT_PROJECT_ID);
    }

    #[test]
    fn closing_the_active_project_moves_to_the_left_neighbour() {
        let mut ws = Workspace::new();
        ws.open_project("id-1");
        ws.open_project("id-2");
        assert_eq!(ws.active(), "id-2");

        assert!(ws.close("id-2"));
        assert_eq!(ws.active(), "id-1", "左隣がアクティブになる");
        assert!(ws.close("id-1"));
        assert_eq!(ws.active(), DEFAULT_PROJECT_ID);
    }

    #[test]
    fn closing_an_inactive_project_keeps_the_active_one() {
        let mut ws = Workspace::new();
        ws.open_project("id-1");
        ws.open_project("id-2");
        ws.switch("id-2");

        assert!(ws.close("id-1"));
        assert_eq!(ws.active(), "id-2");
        assert_eq!(ws.open_ids(), &[DEFAULT_PROJECT_ID.to_string(), "id-2".to_string()]);
    }

    #[test]
    fn the_default_project_can_never_be_closed() {
        let mut ws = Workspace::new();
        ws.open_project("id-1");

        assert!(!ws.close(DEFAULT_PROJECT_ID));
        assert!(ws.is_open(DEFAULT_PROJECT_ID));
        // 開いていないものを閉じようとしても false
        assert!(!ws.close("id-unknown"));
    }

    #[test]
    fn restore_always_keeps_the_default_project_first() {
        let store = store_with(&[("id-1", "Alpha")]);
        let ws = Workspace::restore(&["id-1".to_string()], Some("id-1"), &store);

        assert_eq!(ws.open_ids(), &[DEFAULT_PROJECT_ID.to_string(), "id-1".to_string()]);
        assert_eq!(ws.active(), "id-1");
    }

    #[test]
    fn restore_drops_projects_that_no_longer_exist() {
        let store = store_with(&[("id-1", "Alpha")]);
        let ws = Workspace::restore(
            &["id-1".to_string(), "id-deleted".to_string()],
            Some("id-deleted"),
            &store,
        );

        assert_eq!(ws.open_ids(), &[DEFAULT_PROJECT_ID.to_string(), "id-1".to_string()]);
        // アクティブが消えていた場合はデフォルトへ戻す
        assert_eq!(ws.active(), DEFAULT_PROJECT_ID);
    }

    #[test]
    fn restore_removes_duplicates_and_handles_empty_state() {
        let store = store_with(&[("id-1", "Alpha")]);
        let ws = Workspace::restore(
            &["id-1".to_string(), "id-1".to_string(), DEFAULT_PROJECT_ID.to_string()],
            None,
            &store,
        );

        assert_eq!(ws.open_ids(), &[DEFAULT_PROJECT_ID.to_string(), "id-1".to_string()]);
        assert_eq!(ws.active(), DEFAULT_PROJECT_ID);

        let empty = Workspace::restore(&[], None, &store);
        assert_eq!(empty.open_ids(), &[DEFAULT_PROJECT_ID.to_string()]);
    }

    #[test]
    fn owner_is_the_first_open_project_that_contains_the_sheet() {
        let mut store = store_with(&[("id-1", "Alpha"), ("id-2", "Beta")]);
        // 同じシートを 2 つのプロジェクトへ登録する
        store.toggle_sheet("id-2", "guid-a", T0);
        store.toggle_sheet("id-1", "guid-a", T0);

        let mut ws = Workspace::new();
        ws.open_project("id-1");
        ws.open_project("id-2");

        let keys = vec!["guid-a".to_string()];
        assert_eq!(ws.owner_of(&keys, &store), Some("id-1"), "開いている順で最初のもの");
    }

    #[test]
    fn owner_is_none_when_no_open_project_contains_the_sheet() {
        let mut store = store_with(&[("id-1", "Alpha")]);
        store.toggle_sheet("id-1", "guid-a", T0);

        // id-1 を開いていない状態では所有者なし
        let ws = Workspace::new();
        assert_eq!(ws.owner_of(&["guid-a".to_string()], &store), None);
        // どこにも属さない guid も None
        let mut ws2 = Workspace::new();
        ws2.open_project("id-1");
        assert_eq!(ws2.owner_of(&["guid-other".to_string()], &store), None);
    }

    #[test]
    fn owner_matches_any_of_the_given_key_variants() {
        let mut store = store_with(&[("id-1", "Alpha")]);
        store.toggle_sheet("id-1", "guid-a", T0);
        let mut ws = Workspace::new();
        ws.open_project("id-1");

        // guid は拡張子の有無で表記が揺れるため、候補のいずれかが一致すればよい
        let keys = vec!["guid-a.md".to_string(), "guid-a".to_string()];
        assert_eq!(ws.owner_of(&keys, &store), Some("id-1"));
    }
}
