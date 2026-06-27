!include "LogicLib.nsh"

!define UCP_UNINSTALL_REG_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\dev.ucp.clipboard"
!define UCP_INSTANCE_MUTEX "Local\dev.ucp.clipboard.single-instance"

Function .onInit
    ReadRegStr $0 SHCTX "${UCP_UNINSTALL_REG_KEY}" "InstallLocation"
    ${If} $0 != ""
        StrCpy $INSTDIR "$0"
    ${EndIf}

    Call EnsureUcpNotRunning
FunctionEnd

Function EnsureUcpNotRunning
    retry_check:
    Call IsUcpRunning
    Pop $0
    ${If} $0 == 0
        Return
    ${EndIf}

    MessageBox MB_ICONEXCLAMATION|MB_YESNO \
        "UCP Clipboard 正在运行，安装前需要关闭它。$\r$\n$\r$\n是否立即关闭 UCP Clipboard 并继续安装？" \
        IDYES close_running IDNO abort_install

    close_running:
        Call CloseRunningUcp
        Call IsUcpRunning
        Pop $0
        ${If} $0 == 0
            Return
        ${EndIf}

        MessageBox MB_ICONSTOP|MB_RETRYCANCEL \
            "无法自动关闭 UCP Clipboard。请手动退出后点击重试。" \
            IDRETRY retry_check IDCANCEL abort_install

    abort_install:
        Abort "安装已取消。"
FunctionEnd

Function CloseRunningUcp
    IfFileExists "$INSTDIR\ucp.exe" 0 force_close
    nsExec::ExecToLog '"$INSTDIR\ucp.exe" --quit'
    Pop $0
    Call WaitForUcpExit
    Pop $0
    ${If} $0 == 1
        Return
    ${EndIf}

    force_close:
        nsExec::ExecToLog 'taskkill /IM ucp.exe /T /F'
        Pop $0
        Call WaitForUcpExit
        Pop $0
FunctionEnd

Function WaitForUcpExit
    StrCpy $1 0

    wait_loop:
        Call IsUcpRunning
        Pop $0
        ${If} $0 == 0
            Push 1
            Return
        ${EndIf}

        ${If} $1 >= 50
            Push 0
            Return
        ${EndIf}

        Sleep 200
        IntOp $1 $1 + 1
        Goto wait_loop
FunctionEnd

Function IsUcpRunning
    System::Call 'kernel32::OpenMutexW(i 0x00100000, i 0, w "${UCP_INSTANCE_MUTEX}") p .r0'
    ${If} $0 == 0
        Push 0
    ${Else}
        System::Call 'kernel32::CloseHandle(p r0)'
        Push 1
    ${EndIf}
FunctionEnd
