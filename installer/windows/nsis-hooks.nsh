; Reuse the existing install directory on reinstall/update.
; App updates can run the generated installer with /S and it will overwrite in place.
Function .onInit
    ReadRegStr $0 HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\dev.ucp.clipboard" "InstallLocation"
    StrCmp $0 "" 0 found_install_dir

    ReadRegStr $0 HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\dev.ucp.clipboard" "InstallLocation"
    StrCmp $0 "" done found_install_dir

found_install_dir:
    IfFileExists "$0\ucp.exe" 0 done
    StrCpy $INSTDIR $0

done:
FunctionEnd
