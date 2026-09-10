; skills-manager NSIS installer hooks.
;
; WebView2 stays an ONLINE install (bundle.windows.webviewInstallMode =
; downloadBootstrapper): the installer downloads the ~1.8MB bootstrapper and
; runs it. On healthy machines that is enough.
;
; It is NOT enough on machines in the "ghost registration" state we hit in
; the field (2026-09-10, colleague report): the bootstrapper sees a stale
; per-machine/per-user registration, prints "已为系统安装 Microsoft Edge
; Webview2 Runtime" and exits without installing anything usable — the app
; then dies at startup with "Could not find the WebView2 Runtime".
;
; This hook runs after files are copied and verifies the runtime is actually
; present (registry version + the runtime binary on disk). When it is not,
; the user gets an explanation and a one-click jump to the download page,
; instead of a silent install followed by an unexplained startup error.

!macro NSIS_HOOK_POSTINSTALL
  ; --- locate a registered WebView2 runtime version -------------------
  ; Evergreen Runtime registers under EdgeUpdate\Clients\{F3017226-...};
  ; per-machine lives in HKLM, per-user in HKCU. Check the 64-bit view
  ; first (this app is x64), then the 32-bit view as a fallback.
  StrCpy $0 ""

  SetRegView 64
  ReadRegStr $0 HKLM "SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" "pv"
  ${If} $0 == ""
    ReadRegStr $0 HKCU "SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" "pv"
  ${EndIf}

  ${If} $0 == ""
    SetRegView 32
    ReadRegStr $0 HKLM "SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" "pv"
    ${If} $0 == ""
      ReadRegStr $0 HKCU "SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" "pv"
    ${EndIf}
  ${EndIf}
  SetRegView lastused

  ; 0.0.0.0 is what a botched/removed install leaves behind (and what the
  ; bootstrapper treats as "already installed").
  ${If} $0 == "0.0.0.0"
    StrCpy $0 ""
  ${EndIf}

  ; --- verify the runtime binary actually exists ----------------------
  StrCpy $1 ""
  ${If} $0 != ""
    ; Evergreen deploys to Program Files (x86) even on 64-bit Windows.
    ${If} ${FileExists} "$PROGRAMFILES32\Microsoft\EdgeWebView\Application\$0\msedgewebview2.exe"
      StrCpy $1 "ok"
    ${ElseIf} ${FileExists} "$PROGRAMFILES64\Microsoft\EdgeWebView\Application\$0\msedgewebview2.exe"
      StrCpy $1 "ok"
    ${EndIf}
  ${EndIf}

  ${If} $1 != "ok"
    MessageBox MB_YESNO|MB_ICONEXCLAMATION \
      "Skills Manager 已安装完成，但没有检测到可用的 WebView2 运行时，应用可能无法启动。$\r$\n$\r$\n\
常见原因：系统里存在失效的 WebView2 安装记录（安装程序会误判为“已安装”而跳过）。$\r$\n$\r$\n\
解决办法：$\r$\n\
1. 点击“是”打开官方下载页，下载 Evergreen Standalone Installer (x64)；$\r$\n\
2. 右键“以管理员身份运行”安装，完成后重新打开 Skills Manager；$\r$\n\
3. 若无法安装（被组策略/安全软件拦截），请联系 IT 协助。$\r$\n$\r$\n\
是否现在打开下载页？" \
      IDNO done_webview2_check
    ExecShell "open" "https://developer.microsoft.com/microsoft-edge/webview2/"
  ${EndIf}

  done_webview2_check:
!macroend
