//! Alt+Shift+R（アクティブなシートを Drive から再読み込み）の判定。
//!
//! 再読み込み自体は既存の同期チェック（`trigger_conflict_check`）に任せる。
//! ここでは「そもそも Drive から読み直せるシートか」だけを判定し、
//! 表示や通信から切り離して単体テストの対象にしている。
//! 仕様は docs/SHEET_RELOAD_AND_SWITCHER_BADGE_SPEC.md §1-3。

/// ローカルファイルのシートを表すカテゴリ
const LOCAL_CATEGORY: &str = "__LOCAL__";

/// 再読み込みの対象（アクティブなシートの状態）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReloadTarget<'a> {
    /// ターミナルがアクティブか
    pub terminal_active: bool,
    /// アクティブなシートの Drive ID
    pub drive_id: Option<&'a str>,
    /// アクティブなシートのカテゴリ（`__LOCAL__` はローカルファイル）
    pub category: &'a str,
}

/// Drive から再読み込みできるシートかどうか。
///
/// * ターミナルがアクティブなら false
/// * `drive_id` が無ければ false（未保存の新規シート）
/// * カテゴリが `__LOCAL__` なら false（ローカルファイル）
/// * それ以外（Drive のシート）は true
pub fn can_reload_from_drive(target: &ReloadTarget) -> bool {
    !target.terminal_active && target.drive_id.is_some() && target.category != LOCAL_CATEGORY
}

/// 手動の再読み込みで取得した Drive の本文をどう扱うか
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReloadDecision {
    /// 今の本文と同じ。何もしない（同期時刻だけ追従する）
    NoChange,
    /// Drive の本文で置き換える
    Replace,
    /// 未保存の編集があるので、競合ダイアログで作業者に選ばせる
    Conflict,
}

/// 手動の再読み込み（Alt+Shift+R）で、取得した Drive の本文をどう扱うかを決める。
///
/// バックグラウンドの同期チェックは更新時刻とファイルサイズで推測するが
/// （同じサイズなら名前変更などメタデータだけの変更とみなす）、
/// 1 文字の書き換えのようにサイズが変わらない編集を取りこぼす。
/// 手動の時は本文そのものを比べる。
///
/// * 本文が同じなら NoChange
/// * 違っていて、未保存の編集が無ければ Replace
/// * 違っていて、未保存の編集があれば Conflict
pub fn decide_reload(local: &str, drive: &str, has_local_edit: bool) -> ReloadDecision {
    if local == drive {
        ReloadDecision::NoChange
    } else if has_local_edit {
        ReloadDecision::Conflict
    } else {
        ReloadDecision::Replace
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_content_needs_no_change() {
        assert_eq!(decide_reload("abc", "abc", false), ReloadDecision::NoChange);
        // 未保存の編集フラグが立っていても、内容が Drive と同じなら何もしない
        assert_eq!(decide_reload("abc", "abc", true), ReloadDecision::NoChange);
    }

    #[test]
    fn different_content_without_local_edit_is_replaced() {
        assert_eq!(decide_reload("old", "new", false), ReloadDecision::Replace);
    }

    #[test]
    fn same_size_edit_is_still_detected() {
        // バイト数が変わらない書き換えも取りこぼさない（バックグラウンド同期の弱点）
        assert_eq!(decide_reload("abc", "abd", false), ReloadDecision::Replace);
    }

    #[test]
    fn different_content_with_local_edit_goes_to_conflict() {
        assert_eq!(decide_reload("local edit", "remote edit", true), ReloadDecision::Conflict);
    }

    fn target<'a>(terminal_active: bool, drive_id: Option<&'a str>, category: &'a str) -> ReloadTarget<'a> {
        ReloadTarget { terminal_active, drive_id, category }
    }

    // 規則 4
    #[test]
    fn drive_sheet_can_be_reloaded() {
        assert!(can_reload_from_drive(&target(false, Some("drive-1"), "cat-1")));
        // カテゴリ無し（「カテゴリなし」フォルダ）でも Drive のシートなら可
        assert!(can_reload_from_drive(&target(false, Some("drive-1"), "")));
    }

    // 規則 1
    #[test]
    fn terminal_active_cannot_reload() {
        assert!(!can_reload_from_drive(&target(true, Some("drive-1"), "cat-1")));
    }

    // 規則 2
    #[test]
    fn unsaved_sheet_without_drive_id_cannot_reload() {
        assert!(!can_reload_from_drive(&target(false, None, "cat-1")));
        assert!(!can_reload_from_drive(&target(false, None, "")));
    }

    // 規則 3
    #[test]
    fn local_file_sheet_cannot_reload() {
        assert!(!can_reload_from_drive(&target(false, None, "__LOCAL__")));
        // 万一 drive_id が入っていてもローカルファイルは対象外
        assert!(!can_reload_from_drive(&target(false, Some("drive-1"), "__LOCAL__")));
    }
}
