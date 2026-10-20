; Uninstaller hooks for Umbilical (bundle.windows.nsis.installerHooks).
;
; The sessions run in a daemon (`umbilical.exe --daemon`) that lives on
; after the GUI quits. Stop it cleanly, remove the start-at-login entry, and
; delete our app data when the user picks "Delete the application data".
; Folders, repositories and Remote Control worktrees are never touched.

!macro NSIS_HOOK_PREUNINSTALL
  ; Close the GUI first. Else it starts the daemon again at once.
  nsExec::Exec `powershell.exe -NoProfile -NonInteractive -Command "Get-CimInstance Win32_Process -Filter \"Name='${MAINBINARYNAME}.exe' AND NOT CommandLine LIKE '%--daemon%'\" | Invoke-CimMethod -MethodName Terminate"`
  Pop $0
  ; Stop every session and the daemon, and wait until it is gone.
  nsExec::Exec `"$INSTDIR\${MAINBINARYNAME}.exe" --stop-daemon`
  Pop $0
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ; An update runs the old uninstaller too. Keep everything then.
  ${If} $UpdateMode <> 1
    ; tauri-plugin-autostart writes these values.
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "${PRODUCTNAME}"
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run" "${PRODUCTNAME}"
  ${EndIf}

  ${If} $DeleteAppDataCheckboxState = 1
  ${AndIf} $UpdateMode <> 1
    SetShellVarContext current
    ; Only our own files: the default root .umbilical\repos can hold the
    ; user's repositories. RMDir without /r removes only empty folders.
    Delete "$PROFILE\.umbilical\config.toml"
    Delete "$PROFILE\.umbilical\config.toml.tmp"
    Delete "$PROFILE\.umbilical\daemon.json"
    Delete "$PROFILE\.umbilical\daemon.json.tmp"
    Delete "$PROFILE\.umbilical\daemon.lock"
    Delete "$PROFILE\.umbilical\sessions.json"
    Delete "$PROFILE\.umbilical\sessions.json.tmp"
    Delete "$PROFILE\.umbilical\update-failed"
    RMDir "$PROFILE\.umbilical\repos"
    RMDir "$PROFILE\.umbilical"
    ; The default log folder. Its parent is also the default $INSTDIR.
    RMDir /r "$LOCALAPPDATA\${PRODUCTNAME}\logs"
    RMDir "$LOCALAPPDATA\${PRODUCTNAME}"
  ${EndIf}
!macroend
