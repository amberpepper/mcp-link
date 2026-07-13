!macro NSIS_HOOK_PREUNINSTALL
  ${If} $UpdateMode <> 1
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "MCP Link"
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run" "MCP Link"
    SetShellVarContext current
    RMDir /r "$LOCALAPPDATA\MCP Link"
  ${EndIf}
!macroend
