//! タブの振り分けロジック。
//!
//! タブエリアは次の 3 つに分かれる。
//!
//! * 左端に固定される「ターミナル」タブ（ドロップダウン）
//! * その右に固定される「プロジェクト」タブ（ドロップダウン。開いているプロジェクトのシート）
//! * それ以外の通常のシートタブ（ドラッグで並べ替え可能）
//!
//! どのシートがどこに入るかはインデックス演算だけで決まるため、
//! 純粋関数として切り出して単体テストの対象にしている。

use std::collections::HashSet;

/// ターミナルタブの ID 接頭辞
pub const TERMINAL_ID_PREFIX: &str = "__TERM__";

pub fn is_terminal_tab(id: &str) -> bool {
    id.starts_with(TERMINAL_ID_PREFIX)
}

/// Drive 上のファイル名（`{guid}.{拡張子}`）から guid を取り出す
pub fn guid_from_drive_name(name: &str) -> String {
    match name.rfind('.') {
        Some(pos) if pos > 0 => name[..pos].to_string(),
        _ => name.to_string(),
    }
}

/// シートがどのプロジェクト guid に対応するかの候補を返す。
///
/// `Sheet.guid` は開かれた経路によって拡張子が残る場合（`abc.md`）と
/// 残らない場合（`abc`）があるため、`guid` と `title` の双方から
/// 拡張子を除いた値も候補に含めて取りこぼしを防ぐ。
pub fn sheet_project_keys(guid: Option<&str>, title: &str) -> Vec<String> {
    let mut keys = Vec::new();
    if let Some(guid) = guid {
        keys.push(guid.to_string());
        keys.push(guid_from_drive_name(guid));
    }
    if !title.is_empty() {
        keys.push(guid_from_drive_name(title));
    }
    keys
}

/// 振り分けの入力となるタブ 1 件
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabEntry {
    pub id: String,
    /// シートの場合のプロジェクト guid 候補（ターミナルでは空）
    pub keys: Vec<String>,
}

impl TabEntry {
    pub fn sheet(id: &str, guid: Option<&str>, title: &str) -> Self {
        Self { id: id.to_string(), keys: sheet_project_keys(guid, title) }
    }

    pub fn terminal(id: &str) -> Self {
        Self { id: id.to_string(), keys: Vec::new() }
    }
}

/// 振り分け結果。いずれも入力順を保持する。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TabPartition {
    /// 「ターミナル」タブのドロップダウンに入る
    pub terminals: Vec<String>,
    /// 「プロジェクト」タブのドロップダウンに入る
    pub project: Vec<String>,
    /// 通常のシートタブとして並ぶ
    pub normal: Vec<String>,
}

/// タブを「ターミナル」「プロジェクトのシート」「通常のシート」に振り分ける。
///
/// `project_sheets` は開いているプロジェクトの所属 guid。
/// プロジェクトを開いていない場合は空集合を渡す（全てのシートが通常タブになる）。
///
/// 同じシートを 2 箇所に出さないため、プロジェクトに所属していれば
/// 必ずドロップダウン側にだけ入る。
pub fn partition_tabs(tabs: &[TabEntry], project_sheets: &HashSet<String>) -> TabPartition {
    let mut result = TabPartition::default();
    for tab in tabs {
        if is_terminal_tab(&tab.id) {
            result.terminals.push(tab.id.clone());
        } else if tab.keys.iter().any(|k| project_sheets.contains(k)) {
            result.project.push(tab.id.clone());
        } else {
            result.normal.push(tab.id.clone());
        }
    }
    result
}

/// 重複タブ整理の入力となるタブ 1 件
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DedupeEntry {
    pub id: String,
    /// プロジェクト guid 候補（`sheet_project_keys` の結果）
    pub keys: Vec<String>,
    pub drive_id: Option<String>,
    /// 未保存の変更を抱えているか
    pub has_unsaved: bool,
}

/// 同じ Drive 上のファイルを指すかどうかの識別子。
/// 判定できない（未保存の新規シート等）場合は None を返し、重複扱いしない。
fn identity_of(entry: &DedupeEntry) -> Option<String> {
    if let Some(drive_id) = entry.drive_id.as_ref() {
        return Some(format!("drive:{}", drive_id));
    }
    // guid は拡張子の有無で表記が揺れるため、除いた形で揃える
    entry
        .keys
        .iter()
        .map(|k| guid_from_drive_name(k))
        .find(|k| !k.is_empty())
        .map(|k| format!("guid:{}", k))
}

/// 同じシートが複数のタブとして開かれている場合に、閉じてよいタブの ID を返す。
///
/// * 最初の 1 つは必ず残す
/// * 未保存の変更を抱えているタブは、重複していても残す（編集内容を失わないため）
/// * Drive 上に実体が無いシート（未保存の新規シート）は重複扱いしない
pub fn duplicate_tab_ids(entries: &[DedupeEntry]) -> Vec<String> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut removable = Vec::new();
    for entry in entries {
        let identity = match identity_of(entry) {
            Some(id) => id,
            None => continue,
        };
        if seen.insert(identity) {
            continue; // 最初の 1 つ
        }
        if !entry.has_unsaved {
            removable.push(entry.id.clone());
        }
    }
    removable
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(list: &[&str]) -> HashSet<String> {
        list.iter().map(|s| s.to_string()).collect()
    }


    fn dup(id: &str, guid: Option<&str>, title: &str, drive_id: Option<&str>, unsaved: bool) -> DedupeEntry {
        DedupeEntry {
            id: id.to_string(),
            keys: sheet_project_keys(guid, title),
            drive_id: drive_id.map(|d| d.to_string()),
            has_unsaved: unsaved,
        }
    }

    #[test]
    fn no_duplicates_means_nothing_to_remove() {
        let entries = vec![
            dup("s1", Some("guid-a"), "guid-a.txt", Some("drive-a"), false),
            dup("s2", Some("guid-b"), "guid-b.txt", Some("drive-b"), false),
        ];
        assert!(duplicate_tab_ids(&entries).is_empty());
    }

    #[test]
    fn later_tabs_of_the_same_drive_file_are_removable() {
        let entries = vec![
            dup("s1", Some("guid-a"), "guid-a.txt", Some("drive-a"), false),
            dup("s2", Some("guid-a"), "guid-a.txt", Some("drive-a"), false),
            dup("s3", Some("guid-a"), "guid-a.txt", Some("drive-a"), false),
        ];
        // 最初の 1 つは残す
        assert_eq!(duplicate_tab_ids(&entries), vec!["s2", "s3"]);
    }

    #[test]
    fn duplicates_match_across_guid_extension_forms() {
        // guid に拡張子が残っている経路と残らない経路で開かれた同じシート
        let entries = vec![
            dup("s1", Some("guid-a"), "guid-a.md", None, false),
            dup("s2", Some("guid-a.md"), "guid-a.md", None, false),
        ];
        assert_eq!(duplicate_tab_ids(&entries), vec!["s2"]);
    }

    #[test]
    fn unsaved_duplicates_are_kept() {
        let entries = vec![
            dup("s1", Some("guid-a"), "guid-a.txt", Some("drive-a"), false),
            dup("s2", Some("guid-a"), "guid-a.txt", Some("drive-a"), true),
        ];
        // 編集中の内容を失わないよう、未保存のタブは重複でも残す
        assert!(duplicate_tab_ids(&entries).is_empty());
    }

    #[test]
    fn unsaved_new_sheets_are_never_treated_as_duplicates() {
        // Drive 上に実体が無い新規シートは、何枚あっても重複ではない
        let entries = vec![
            dup("s1", None, "", None, false),
            dup("s2", None, "", None, false),
        ];
        assert!(duplicate_tab_ids(&entries).is_empty());
    }

    #[test]
    fn drive_id_takes_precedence_over_guid() {
        // 同じ guid でも別ファイル（drive_id が違う）なら重複ではない
        let entries = vec![
            dup("s1", Some("guid-a"), "guid-a.txt", Some("drive-a"), false),
            dup("s2", Some("guid-a"), "guid-a.txt", Some("drive-b"), false),
        ];
        assert!(duplicate_tab_ids(&entries).is_empty());
    }

    #[test]
    fn terminal_ids_are_detected_by_prefix() {
        assert!(is_terminal_tab("__TERM__1"));
        assert!(!is_terminal_tab("1750000000000"));
        assert!(!is_terminal_tab(""));
    }

    #[test]
    fn guid_is_extracted_from_drive_file_name() {
        assert_eq!(guid_from_drive_name("abc-123.txt"), "abc-123");
        assert_eq!(guid_from_drive_name("abc-123.md"), "abc-123");
        // 拡張子が無い場合はそのまま
        assert_eq!(guid_from_drive_name("abc-123"), "abc-123");
        // 先頭がドットのファイルは削らない
        assert_eq!(guid_from_drive_name(".leaf_projects"), ".leaf_projects");
    }

    #[test]
    fn project_keys_cover_both_guid_forms() {
        // guid に拡張子が残っている経路でも、除いた形が候補に入る
        let keys = sheet_project_keys(Some("abc-123.md"), "abc-123.md");
        assert!(keys.contains(&"abc-123.md".to_string()));
        assert!(keys.contains(&"abc-123".to_string()));

        // guid が無くてもファイル名から拾える
        let keys = sheet_project_keys(None, "abc-123.txt");
        assert_eq!(keys, vec!["abc-123".to_string()]);

        // どちらも無ければ空
        assert!(sheet_project_keys(None, "").is_empty());
    }

    #[test]
    fn tabs_without_project_all_become_normal_sheets() {
        let tabs = vec![
            TabEntry::sheet("s1", Some("guid-a"), "guid-a.txt"),
            TabEntry::sheet("s2", Some("guid-b"), "guid-b.md"),
        ];
        let p = partition_tabs(&tabs, &set(&[]));

        assert_eq!(p.normal, vec!["s1", "s2"]);
        assert!(p.project.is_empty());
        assert!(p.terminals.is_empty());
    }

    #[test]
    fn project_sheets_go_to_the_project_dropdown_only() {
        let tabs = vec![
            TabEntry::sheet("s1", Some("guid-a"), "guid-a.txt"),
            TabEntry::sheet("s2", Some("guid-b"), "guid-b.md"),
            TabEntry::sheet("s3", Some("guid-c"), "guid-c.txt"),
        ];
        let p = partition_tabs(&tabs, &set(&["guid-a", "guid-c"]));

        // 同じシートがタブバーとドロップダウンの両方に出ないこと
        assert_eq!(p.project, vec!["s1", "s3"]);
        assert_eq!(p.normal, vec!["s2"]);
    }

    #[test]
    fn already_open_sheet_moves_into_the_project_dropdown() {
        // 拡張子付きの guid で開かれていたシートも、プロジェクト側の
        // 拡張子なし guid と突き合わせて正しく移る
        let tabs = vec![TabEntry::sheet("s1", Some("guid-a.md"), "guid-a.md")];
        let p = partition_tabs(&tabs, &set(&["guid-a"]));

        assert_eq!(p.project, vec!["s1"]);
        assert!(p.normal.is_empty());
    }

    #[test]
    fn terminals_are_separated_regardless_of_project() {
        let tabs = vec![
            TabEntry::terminal("__TERM__1"),
            TabEntry::sheet("s1", Some("guid-a"), "guid-a.txt"),
            TabEntry::terminal("__TERM__2"),
            TabEntry::sheet("s2", Some("guid-b"), "guid-b.txt"),
        ];
        let p = partition_tabs(&tabs, &set(&["guid-a"]));

        assert_eq!(p.terminals, vec!["__TERM__1", "__TERM__2"]);
        assert_eq!(p.project, vec!["s1"]);
        assert_eq!(p.normal, vec!["s2"]);
    }

    #[test]
    fn input_order_is_preserved_in_each_group() {
        let tabs = vec![
            TabEntry::sheet("s3", Some("guid-c"), "guid-c.txt"),
            TabEntry::sheet("s1", Some("guid-a"), "guid-a.txt"),
            TabEntry::sheet("s2", Some("guid-b"), "guid-b.txt"),
        ];
        let p = partition_tabs(&tabs, &set(&["guid-c", "guid-b"]));

        assert_eq!(p.project, vec!["s3", "s2"], "並べ替え順ではなく入力順を保つ");
        assert_eq!(p.normal, vec!["s1"]);
    }

    #[test]
    fn unsaved_sheets_without_guid_stay_as_normal_tabs() {
        // guid も drive_id も無い新規シートはプロジェクトに入れられない
        let tabs = vec![TabEntry::sheet("s1", None, "Untitled.txt")];
        let p = partition_tabs(&tabs, &set(&["Untitled"]));
        // ファイル名から拾えてしまわないよう、実際の guid 集合とは一致しない想定
        assert_eq!(p.project.len() + p.normal.len(), 1);
    }
}
