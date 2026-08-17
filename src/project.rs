//! プロジェクト機能のデータモデル。
//!
//! Google ドライブのアプリケーションフォルダに置く `.leaf_projects.json` の
//! 内容そのものを表す。UI と Drive アクセスから切り離した純粋なロジックのみを持ち、
//! `cargo test`（ネイティブ）で検証できるようにしてある。
//!
//! GUID の採番と現在時刻は副作用（JS 依存）になるため、呼び出し側から引数で受け取る。

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

/// Drive 上の設定ファイル名。先頭がドットのためシート一覧には現れない。
pub const PROJECTS_FILE_NAME: &str = ".leaf_projects.json";

const SCHEMA_VERSION: u32 = 1;

fn default_version() -> u32 {
    SCHEMA_VERSION
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Project {
    /// プロジェクトの GUID（シートと同じく GUID で管理する）
    pub id: String,
    /// プロジェクト名。ストア内でユニーク。
    pub name: String,
    /// 自由記述のメモ。一覧でプロジェクト名の下に表示する。ユニーク制約は無い。
    #[serde(default)]
    pub memo: String,
    /// 所属シートの `Sheet.guid`。追加順を保持し、タブを開く順序になる。
    #[serde(default)]
    pub sheets: Vec<String>,
    #[serde(default)]
    pub created_at: u64,
    #[serde(default)]
    pub updated_at: u64,
}

impl Project {
    /// 開けるプロジェクトかどうか。
    /// シートが 1 件も無いプロジェクトを開くと、全タブを閉じた結果として
    /// 空のシートが 1 枚できるだけで意味がないため開けないようにする。
    pub fn is_openable(&self) -> bool {
        !self.sheets.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectStore {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub projects: Vec<Project>,
    /// guid → 本文プレビュー（先頭 5 行）の**キャッシュ**。
    ///
    /// Drive 上のファイル名は `{guid}.{拡張子}` で内容が分からないため、
    /// シート選択ダイアログが取得済みの本文から先頭 5 行を控えておき、
    /// プロジェクトダイアログの一覧表示に使う。正本はあくまで `sheets` の guid で、
    /// ここが欠けていても機能に影響はない。
    #[serde(default)]
    pub previews: BTreeMap<String, String>,
}

impl Default for ProjectStore {
    fn default() -> Self {
        Self { version: SCHEMA_VERSION, projects: Vec::new(), previews: BTreeMap::new() }
    }
}

/// 本文から一覧表示用のプレビュー（先頭 5 行）を作る。
/// 行数だけでなく 1 行の長さも抑え、設定ファイルが肥大化しないようにする。
pub fn preview_from_content(content: &str) -> String {
    const MAX_LINES: usize = 5;
    const MAX_CHARS_PER_LINE: usize = 80;
    content
        .lines()
        .take(MAX_LINES)
        .map(|line| line.chars().take(MAX_CHARS_PER_LINE).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
        .trim_end()
        .to_string()
}

/// プロジェクト名の検証エラー
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameError {
    /// 空文字または空白のみ
    Empty,
    /// 既存プロジェクトと重複
    Duplicate,
}

impl NameError {
    /// 表示用メッセージの i18n キー
    pub fn i18n_key(self) -> &'static str {
        match self {
            NameError::Empty => "project_name_empty",
            NameError::Duplicate => "project_name_duplicate",
        }
    }
}

/// 名前の同一性判定に使う正規化。前後の空白のみを除去する。
/// 大文字小文字は区別するため、`Project` と `project` は別のプロジェクトとして共存できる。
fn normalize(name: &str) -> String {
    name.trim().to_string()
}

impl ProjectStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// JSON から復元する。壊れている場合はデータ消失を避けるためエラーにせず空として扱う。
    pub fn from_json(json: &str) -> Self {
        serde_json::from_str(json).unwrap_or_default()
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{\"version\":1,\"projects\":[]}".to_string())
    }

    pub fn find(&self, id: &str) -> Option<&Project> {
        self.projects.iter().find(|p| p.id == id)
    }

    fn find_mut(&mut self, id: &str) -> Option<&mut Project> {
        self.projects.iter_mut().find(|p| p.id == id)
    }

    /// プロジェクト名を検証し、保存に使う正規化済み（trim 済み）の名前を返す。
    /// `exclude_id` に指定したプロジェクトは重複判定から除外する（改名時に自分自身を許すため）。
    pub fn validate_name(&self, name: &str, exclude_id: Option<&str>) -> Result<String, NameError> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err(NameError::Empty);
        }
        let key = normalize(trimmed);
        let duplicated = self
            .projects
            .iter()
            .filter(|p| Some(p.id.as_str()) != exclude_id)
            .any(|p| normalize(&p.name) == key);
        if duplicated {
            return Err(NameError::Duplicate);
        }
        Ok(trimmed.to_string())
    }

    /// プロジェクトを追加する。`id` と `now` は呼び出し側が生成して渡す。
    pub fn add(&mut self, name: &str, memo: &str, id: &str, now: u64) -> Result<Project, NameError> {
        let name = self.validate_name(name, None)?;
        let project = Project {
            id: id.to_string(),
            name,
            memo: memo.trim().to_string(),
            sheets: Vec::new(),
            created_at: now,
            updated_at: now,
        };
        self.projects.push(project.clone());
        Ok(project)
    }

    /// プロジェクト名とメモを更新する。存在しない ID の場合は何もせず Ok を返す。
    pub fn update(&mut self, id: &str, new_name: &str, memo: &str, now: u64) -> Result<(), NameError> {
        let name = self.validate_name(new_name, Some(id))?;
        if let Some(project) = self.find_mut(id) {
            project.name = name;
            project.memo = memo.trim().to_string();
            project.updated_at = now;
        }
        Ok(())
    }

    /// プロジェクトを削除する。削除した場合のみ true。シート本体には影響しない。
    pub fn remove(&mut self, id: &str) -> bool {
        let before = self.projects.len();
        self.projects.retain(|p| p.id != id);
        self.projects.len() != before
    }

    /// 指定プロジェクトが開けるか。存在しない ID の場合も false。
    pub fn is_openable(&self, project_id: &str) -> bool {
        self.find(project_id).map(|p| p.is_openable()).unwrap_or(false)
    }

    pub fn contains_sheet(&self, project_id: &str, sheet_guid: &str) -> bool {
        self.find(project_id)
            .map(|p| p.sheets.iter().any(|s| s == sheet_guid))
            .unwrap_or(false)
    }

    /// シートの所属を切り替える。追加したら `Some(true)`、外したら `Some(false)`、
    /// プロジェクトが見つからなければ `None`。
    pub fn toggle_sheet(&mut self, project_id: &str, sheet_guid: &str, now: u64) -> Option<bool> {
        let project = self.find_mut(project_id)?;
        if let Some(pos) = project.sheets.iter().position(|s| s == sheet_guid) {
            project.sheets.remove(pos);
            project.updated_at = now;
            Some(false)
        } else {
            project.sheets.push(sheet_guid.to_string());
            project.updated_at = now;
            Some(true)
        }
    }

    /// シートをプロジェクトから外す。外した場合のみ true。
    pub fn remove_sheet(&mut self, project_id: &str, sheet_guid: &str, now: u64) -> bool {
        match self.find_mut(project_id) {
            Some(project) => {
                let before = project.sheets.len();
                project.sheets.retain(|s| s != sheet_guid);
                let changed = project.sheets.len() != before;
                if changed {
                    project.updated_at = now;
                }
                changed
            }
            None => false,
        }
    }

    /// 本文プレビューのキャッシュを更新する。
    /// 空文字は無視する（古いプレビューを空で潰さないため）。
    /// 内容が変わっていない場合は false を返し、無駄な保存を避ける。
    pub fn set_sheet_preview(&mut self, sheet_guid: &str, content: &str) -> bool {
        let preview = preview_from_content(content);
        if preview.trim().is_empty() {
            return false;
        }
        if self.previews.get(sheet_guid) == Some(&preview) {
            return false;
        }
        self.previews.insert(sheet_guid.to_string(), preview);
        true
    }

    /// そのシートが所属している全プロジェクト
    pub fn projects_for_sheet(&self, sheet_guid: &str) -> Vec<&Project> {
        self.projects
            .iter()
            .filter(|p| p.sheets.iter().any(|s| s == sheet_guid))
            .collect()
    }

    /// Drive 上に存在しなくなったシートを全プロジェクトから取り除く。
    /// 変更があった場合のみ true（呼び出し側が保存要否を判断できるようにする）。
    pub fn prune_missing_sheets(&mut self, existing: &HashSet<String>, now: u64) -> bool {
        let mut changed = false;
        for project in self.projects.iter_mut() {
            let before = project.sheets.len();
            project.sheets.retain(|s| existing.contains(s));
            if project.sheets.len() != before {
                project.updated_at = now;
                changed = true;
            }
        }
        // どのプロジェクトからも参照されなくなったプレビューキャッシュを捨てる
        let referenced: HashSet<&String> =
            self.projects.iter().flat_map(|p| p.sheets.iter()).collect();
        let before_previews = self.previews.len();
        self.previews.retain(|guid, _| referenced.contains(guid));
        if self.previews.len() != before_previews {
            changed = true;
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: u64 = 1_750_000_000_000;
    const T1: u64 = 1_750_000_001_000;

    fn store_with(names: &[(&str, &str)]) -> ProjectStore {
        let mut store = ProjectStore::new();
        for (id, name) in names {
            store.add(name, "", id, T0).expect("setup add should succeed");
        }
        store
    }

    #[test]
    fn json_round_trip_preserves_memo() {
        let mut store = ProjectStore::new();
        store.add("Alpha", "次期リリース用", "id-1", T0).unwrap();

        let restored = ProjectStore::from_json(&store.to_json());
        assert_eq!(restored.find("id-1").unwrap().memo, "次期リリース用");
    }

    #[test]
    fn add_registers_project_with_given_id() {
        let mut store = ProjectStore::new();
        let project = store.add("Alpha", "", "id-1", T0).unwrap();

        assert_eq!(project.id, "id-1");
        assert_eq!(project.name, "Alpha");
        assert!(project.sheets.is_empty());
        assert_eq!(project.created_at, T0);
        assert_eq!(store.projects.len(), 1);
    }

    #[test]
    fn add_stores_memo() {
        let mut store = ProjectStore::new();
        let project = store.add("Alpha", "  次期リリース用  ", "id-1", T0).unwrap();

        assert_eq!(project.memo, "次期リリース用", "前後の空白は除去される");
        assert_eq!(store.find("id-1").unwrap().memo, "次期リリース用");
    }

    #[test]
    fn memo_is_optional_and_has_no_unique_constraint() {
        let mut store = ProjectStore::new();
        assert!(store.add("Alpha", "", "id-1", T0).is_ok());
        // メモが同じでも名前が違えば作成できる
        assert!(store.add("Beta", "同じメモ", "id-2", T0).is_ok());
        assert!(store.add("Gamma", "同じメモ", "id-3", T0).is_ok());
        assert_eq!(store.find("id-1").unwrap().memo, "");
    }

    #[test]
    fn update_changes_name_and_memo_together() {
        let mut store = ProjectStore::new();
        store.add("Alpha", "旧メモ", "id-1", T0).unwrap();

        assert!(store.update("id-1", "Beta", "新メモ", T1).is_ok());
        let project = store.find("id-1").unwrap();
        assert_eq!(project.name, "Beta");
        assert_eq!(project.memo, "新メモ");
        assert_eq!(project.updated_at, T1);
    }

    #[test]
    fn update_can_clear_memo() {
        let mut store = ProjectStore::new();
        store.add("Alpha", "旧メモ", "id-1", T0).unwrap();

        assert!(store.update("id-1", "Alpha", "", T1).is_ok());
        assert_eq!(store.find("id-1").unwrap().memo, "");
    }

    #[test]
    fn memo_defaults_to_empty_when_absent_in_json() {
        let json = r#"{ "projects": [ { "id": "id-1", "name": "Alpha" } ] }"#;
        let store = ProjectStore::from_json(json);
        assert_eq!(store.find("id-1").unwrap().memo, "");
    }

    #[test]
    fn add_rejects_empty_name() {
        let mut store = ProjectStore::new();
        assert_eq!(store.add("", "", "id-1", T0), Err(NameError::Empty));
        assert_eq!(store.add("   ", "", "id-2", T0), Err(NameError::Empty));
        assert!(store.projects.is_empty());
    }

    #[test]
    fn add_rejects_duplicate_name() {
        let mut store = store_with(&[("id-1", "Alpha")]);
        assert_eq!(store.add("Alpha", "", "id-2", T0), Err(NameError::Duplicate));
        assert_eq!(store.projects.len(), 1);
    }

    #[test]
    fn duplicate_check_is_case_sensitive() {
        let mut store = store_with(&[("id-1", "Alpha")]);
        // 大文字小文字が違えば別のプロジェクトとして共存できる
        assert!(store.add("alpha", "", "id-2", T0).is_ok());
        assert!(store.add("ALPHA", "", "id-3", T0).is_ok());
        assert_eq!(store.projects.len(), 3);
        // 完全一致のみ重複
        assert_eq!(store.add("Alpha", "", "id-4", T0), Err(NameError::Duplicate));
    }

    #[test]
    fn name_is_trimmed_and_duplicate_check_ignores_surrounding_spaces() {
        let mut store = ProjectStore::new();
        let project = store.add("  Alpha  ", "", "id-1", T0).unwrap();
        assert_eq!(project.name, "Alpha", "保存時に前後の空白は除去される");
        assert_eq!(store.add(" Alpha ", "", "id-2", T0), Err(NameError::Duplicate));
        // 空白を除いても別名なら追加できる
        assert!(store.add(" alpha ", "", "id-3", T0).is_ok());
    }

    #[test]
    fn rename_updates_name_and_timestamp() {
        let mut store = store_with(&[("id-1", "Alpha")]);
        assert!(store.update("id-1", "Beta", "", T1).is_ok());

        let project = store.find("id-1").unwrap();
        assert_eq!(project.name, "Beta");
        assert_eq!(project.updated_at, T1);
        assert_eq!(project.created_at, T0, "作成日時は変わらない");
    }

    #[test]
    fn rename_to_own_current_name_is_allowed() {
        let mut store = store_with(&[("id-1", "Alpha")]);
        assert!(store.update("id-1", "Alpha", "", T1).is_ok());
        // 大文字小文字だけを変える改名（Alpha → ALPHA）もできる
        assert!(store.update("id-1", "ALPHA", "", T1).is_ok());
        assert_eq!(store.find("id-1").unwrap().name, "ALPHA");
    }

    #[test]
    fn rename_rejects_other_projects_name() {
        let mut store = store_with(&[("id-1", "Alpha"), ("id-2", "Beta")]);
        assert_eq!(store.update("id-2", "Alpha", "", T1), Err(NameError::Duplicate));
        assert_eq!(store.find("id-2").unwrap().name, "Beta");
    }

    #[test]
    fn remove_deletes_only_the_target() {
        let mut store = store_with(&[("id-1", "Alpha"), ("id-2", "Beta")]);

        assert!(store.remove("id-1"));
        assert!(store.find("id-1").is_none());
        assert!(store.find("id-2").is_some());
        // 存在しない ID では false
        assert!(!store.remove("id-unknown"));
    }

    #[test]
    fn toggle_sheet_adds_then_removes() {
        let mut store = store_with(&[("id-1", "Alpha")]);

        assert_eq!(store.toggle_sheet("id-1", "guid-a", T1), Some(true));
        assert!(store.contains_sheet("id-1", "guid-a"));

        assert_eq!(store.toggle_sheet("id-1", "guid-a", T1), Some(false));
        assert!(!store.contains_sheet("id-1", "guid-a"));
    }

    #[test]
    fn toggle_sheet_returns_none_for_unknown_project() {
        let mut store = ProjectStore::new();
        assert_eq!(store.toggle_sheet("id-unknown", "guid-a", T1), None);
    }

    #[test]
    fn sheets_keep_insertion_order_without_duplicates() {
        let mut store = store_with(&[("id-1", "Alpha")]);
        store.toggle_sheet("id-1", "guid-a", T1);
        store.toggle_sheet("id-1", "guid-b", T1);
        store.toggle_sheet("id-1", "guid-c", T1);
        // 既に所属しているシートを再度トグルすると外れるだけで、重複登録はされない
        store.toggle_sheet("id-1", "guid-b", T1);
        store.toggle_sheet("id-1", "guid-b", T1);

        assert_eq!(store.find("id-1").unwrap().sheets, vec!["guid-a", "guid-c", "guid-b"]);
    }

    #[test]
    fn project_without_sheets_cannot_be_opened() {
        let mut store = store_with(&[("id-1", "Alpha")]);
        assert!(!store.is_openable("id-1"), "作成直後はシートが無いので開けない");

        store.toggle_sheet("id-1", "guid-a", T1);
        assert!(store.is_openable("id-1"), "シートを追加すると開ける");

        // 最後の 1 件を外すと再び開けなくなる
        store.toggle_sheet("id-1", "guid-a", T1);
        assert!(!store.is_openable("id-1"));
    }

    #[test]
    fn unknown_project_is_not_openable() {
        let store = ProjectStore::new();
        assert!(!store.is_openable("id-unknown"));
    }

    #[test]
    fn pruning_all_sheets_makes_project_not_openable() {
        let mut store = store_with(&[("id-1", "Alpha")]);
        store.toggle_sheet("id-1", "guid-gone", T0);
        assert!(store.is_openable("id-1"));

        // Drive 上からシートが消えた場合も開けなくなる
        store.prune_missing_sheets(&HashSet::new(), T1);
        assert!(!store.is_openable("id-1"));
    }

    #[test]
    fn remove_sheet_detaches_without_touching_others() {
        let mut store = store_with(&[("id-1", "Alpha")]);
        store.toggle_sheet("id-1", "guid-a", T0);
        store.toggle_sheet("id-1", "guid-b", T0);

        assert!(store.remove_sheet("id-1", "guid-a", T1));
        assert_eq!(store.find("id-1").unwrap().sheets, vec!["guid-b"]);
        // 所属していないシートでは false
        assert!(!store.remove_sheet("id-1", "guid-a", T1));
    }

    #[test]
    fn projects_for_sheet_returns_every_owner() {
        let mut store = store_with(&[("id-1", "Alpha"), ("id-2", "Beta"), ("id-3", "Gamma")]);
        store.toggle_sheet("id-1", "guid-a", T0);
        store.toggle_sheet("id-3", "guid-a", T0);
        store.toggle_sheet("id-2", "guid-b", T0);

        let owners: Vec<&str> = store
            .projects_for_sheet("guid-a")
            .iter()
            .map(|p| p.id.as_str())
            .collect();
        assert_eq!(owners, vec!["id-1", "id-3"]);
        assert!(store.projects_for_sheet("guid-unknown").is_empty());
    }

    #[test]
    fn prune_missing_sheets_keeps_only_existing_guids() {
        let mut store = store_with(&[("id-1", "Alpha")]);
        store.toggle_sheet("id-1", "guid-a", T0);
        store.toggle_sheet("id-1", "guid-gone", T0);
        store.toggle_sheet("id-1", "guid-b", T0);

        let existing: HashSet<String> =
            ["guid-a", "guid-b"].iter().map(|s| s.to_string()).collect();

        assert!(store.prune_missing_sheets(&existing, T1));
        assert_eq!(store.find("id-1").unwrap().sheets, vec!["guid-a", "guid-b"]);
        // 変化が無ければ false（無駄な保存を避ける）
        assert!(!store.prune_missing_sheets(&existing, T1));
    }

    #[test]
    fn preview_keeps_only_first_five_lines() {
        let content = "1行目\n2行目\n3行目\n4行目\n5行目\n6行目\n7行目";
        assert_eq!(preview_from_content(content), "1行目\n2行目\n3行目\n4行目\n5行目");
    }

    #[test]
    fn preview_truncates_long_lines() {
        let long = "あ".repeat(200);
        let preview = preview_from_content(&long);
        assert_eq!(preview.chars().count(), 80, "1行あたり80文字で打ち切る");
    }

    #[test]
    fn preview_of_short_content_is_kept_as_is() {
        assert_eq!(preview_from_content("ひとこと"), "ひとこと");
        assert_eq!(preview_from_content(""), "");
        // 末尾の空行は落とすが、行内の空行は保持する
        assert_eq!(preview_from_content("a\n\nb\n\n\n\n"), "a\n\nb");
    }

    #[test]
    fn preview_cache_stores_and_reads_back() {
        let mut store = store_with(&[("id-1", "Alpha")]);
        store.toggle_sheet("id-1", "guid-a", T0);

        assert!(store.set_sheet_preview("guid-a", "会議メモ\n・議題\n・決定事項"));
        assert_eq!(store.previews.get("guid-a").map(|s| s.as_str()), Some("会議メモ\n・議題\n・決定事項"));
        assert_eq!(store.previews.get("guid-unknown"), None);
    }

    #[test]
    fn unchanged_preview_reports_no_change() {
        let mut store = ProjectStore::new();
        assert!(store.set_sheet_preview("guid-a", "同じ内容"));
        // 2回目は変化がないので保存不要
        assert!(!store.set_sheet_preview("guid-a", "同じ内容"));
        assert!(store.set_sheet_preview("guid-a", "違う内容"));
    }

    #[test]
    fn empty_preview_does_not_overwrite_cache() {
        let mut store = ProjectStore::new();
        store.set_sheet_preview("guid-a", "本文あり");
        // 空の本文で既存のプレビューを潰さない（読み込み前の空データ対策）
        assert!(!store.set_sheet_preview("guid-a", "   \n \n"));
        assert_eq!(store.previews.get("guid-a").map(|s| s.as_str()), Some("本文あり"));
    }

    #[test]
    fn prune_drops_previews_no_longer_referenced() {
        let mut store = store_with(&[("id-1", "Alpha")]);
        store.toggle_sheet("id-1", "guid-a", T0);
        store.toggle_sheet("id-1", "guid-gone", T0);
        store.set_sheet_preview("guid-a", "残る");
        store.set_sheet_preview("guid-gone", "消える");

        let existing: HashSet<String> = ["guid-a"].iter().map(|s| s.to_string()).collect();
        assert!(store.prune_missing_sheets(&existing, T1));

        assert_eq!(store.previews.get("guid-a").map(|s| s.as_str()), Some("残る"));
        assert_eq!(store.previews.get("guid-gone"), None);
    }

    #[test]
    fn json_round_trip_preserves_content() {
        let mut store = store_with(&[("id-1", "Alpha"), ("id-2", "Beta")]);
        store.toggle_sheet("id-1", "guid-a", T1);
        store.set_sheet_preview("guid-a", "会議メモ\n・議題");

        let restored = ProjectStore::from_json(&store.to_json());
        assert_eq!(restored, store);
    }

    #[test]
    fn unknown_fields_are_ignored_for_forward_compatibility() {
        let json = r##"{
            "version": 2,
            "future_flag": true,
            "projects": [
                { "id": "id-1", "name": "Alpha", "sheets": ["guid-a"], "color": "#fff" }
            ]
        }"##;
        let store = ProjectStore::from_json(json);

        assert_eq!(store.projects.len(), 1);
        assert_eq!(store.find("id-1").unwrap().sheets, vec!["guid-a"]);
    }

    #[test]
    fn missing_optional_fields_fall_back_to_defaults() {
        let json = r#"{ "projects": [ { "id": "id-1", "name": "Alpha" } ] }"#;
        let store = ProjectStore::from_json(json);

        let project = store.find("id-1").unwrap();
        assert!(project.sheets.is_empty());
        assert_eq!(project.created_at, 0);
        assert_eq!(store.version, SCHEMA_VERSION);
    }

    #[test]
    fn broken_json_is_treated_as_empty_store() {
        // 壊れたファイルで操作不能にならないよう、空として扱う
        assert!(ProjectStore::from_json("").projects.is_empty());
        assert!(ProjectStore::from_json("{ broken").projects.is_empty());
        assert!(ProjectStore::from_json("[]").projects.is_empty());
    }
}
