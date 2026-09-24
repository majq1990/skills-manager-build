//! Windows 会话结束（关机/注销/重启）守卫。
//!
//! tao 0.34.5 的窗口子类 proc 在收到 `WM_ENDSESSION` 时会直接调用
//! `loop_destroyed()`，把事件循环的 runner 状态永久停在 `Destroyed`
//! （`run()` 末尾那次调用后面紧跟 `reset_runner()` 复位，唯独这条路径没有）。
//! 之后任何一次事件派发都会撞上 `move_state_to` 的
//! `(Destroyed, _) => panic!("cannot move state from Destroyed")`——
//! 2026-09-18 上午（应用正忙）和 2026-09-24 傍晚（空闲 48 分钟后）两次崩溃
//! 都是这条路径，与用户点退出无关。
//!
//! tao 没法就地修，所以给每个顶层窗口装一个自己的子类 proc。子类链是后进先出，
//! 但这个 guard 在两种调用顺序下都能安全收场：
//! * 我们先看到 `WM_ENDSESSION`：吞掉它（tao 的 proc 根本不会跑），并
//!   `PostQuitMessage(0)`，消息循环从 `GetMessageW` 失败这条正常路径退出；
//! * tao 的 proc 先跑（状态已经 `Destroyed`）：我们再 `PostQuitMessage(0)`，
//!   循环退出时 `loop_destroyed()` 从 `Destroyed` 到 `Destroyed` 是空操作，
//!   `reset_runner()` 会把状态复位成 `Uninitialized`，`run()` 正常返回，
//!   中间不会再有任何派发，也就不会 panic。
//! 两种顺序都是正常拆窗退出，不再弹崩溃框。

#[cfg(windows)]
mod imp {
    use std::sync::atomic::{AtomicBool, Ordering};

    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
    use windows::Win32::UI::WindowsAndMessaging::PostQuitMessage;

    /// Windows 在会话结束时广播给顶层窗口；`wParam != 0` 表示会话真的在结束，
    /// `wParam == 0` 表示注销被取消，那时必须放行、不能退。
    const WM_ENDSESSION: u32 = 0x0016;
    const SUBCLASS_ID: usize = 1;

    static LOGGED: AtomicBool = AtomicBool::new(false);

    /// 给一组顶层窗口句柄安装守卫。重复安装是安全的（同 id 会替换旧 proc），
    /// 但仍然只打一次日志。
    pub fn install(hwnds: Vec<HWND>) {
        for hwnd in hwnds {
            // SAFETY: setup() 跑在主线程上，即这些窗口的属主线程；proc 是一个
            // 普通 fn item，不捕获任何环境。
            let ok = unsafe { SetWindowSubclass(hwnd, Some(session_guard_proc), SUBCLASS_ID, 0) };
            if !ok.as_bool() {
                log::warn!(
                    "session-end guard: SetWindowSubclass failed for hwnd {:?}",
                    hwnd
                );
            }
        }
    }

    unsafe extern "system" fn session_guard_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        _id: usize,
        _data: usize,
    ) -> LRESULT {
        if msg == WM_ENDSESSION && wparam.0 != 0 {
            if !LOGGED.swap(true, Ordering::SeqCst) {
                log::warn!(
                    "Windows 会话结束 (WM_ENDSESSION)：走正常退出路径，规避 tao 退出 panic"
                );
            }
            // SAFETY: 只操作当前线程的消息队列。
            unsafe { PostQuitMessage(0) };
            // 故意不调 DefSubclassProc：吞掉消息，tao 的 proc 就不会把 runner
            // 状态带进 Destroyed。
            return LRESULT(0);
        }
        // SAFETY: 原样把消息交给链上的下一个 proc（tao 的）。
        unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
    }
}

/// 给应用所有顶层窗口安装会话结束守卫。非 Windows 平台是空操作。
pub fn install(app: &tauri::AppHandle) {
    #[cfg(windows)]
    {
        use tauri::Manager;
        let mut hwnds = Vec::new();
        for window in app.webview_windows().values() {
            match window.hwnd() {
                Ok(hwnd) => hwnds.push(hwnd),
                Err(err) => {
                    log::warn!("session-end guard: 取窗口句柄失败: {err}");
                }
            }
        }
        if hwnds.is_empty() {
            log::warn!("session-end guard: 没有拿到任何顶层窗口句柄，跳过安装");
            return;
        }
        imp::install(hwnds);
    }
    #[cfg(not(windows))]
    {
        let _ = app;
    }
}
