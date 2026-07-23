; Static shell verbs for classic context menu / "Show more options".
; Modern Windows 11 top-level menu requires a separate COM sparse package.

!macro NSIS_HOOK_POSTINSTALL
  WriteRegStr HKCU "Software\Classes\*\shell\PrintAssist" "" "使用打印助手打印"
  WriteRegStr HKCU "Software\Classes\*\shell\PrintAssist" "Icon" "$INSTDIR\PrintAssist.exe"
  WriteRegStr HKCU "Software\Classes\*\shell\PrintAssist\command" "" '"$INSTDIR\PrintAssist.exe" "%1"'

  WriteRegStr HKCU "Software\Classes\Directory\shell\PrintAssist" "" "使用打印助手打印文件夹"
  WriteRegStr HKCU "Software\Classes\Directory\shell\PrintAssist" "Icon" "$INSTDIR\PrintAssist.exe"
  WriteRegStr HKCU "Software\Classes\Directory\shell\PrintAssist\command" "" '"$INSTDIR\PrintAssist.exe" "%1"'

  CreateShortCut "$SENDTO\打印助手.lnk" "$INSTDIR\PrintAssist.exe"
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  DeleteRegKey HKCU "Software\Classes\*\shell\PrintAssist"
  DeleteRegKey HKCU "Software\Classes\Directory\shell\PrintAssist"
  Delete "$SENDTO\打印助手.lnk"
!macroend
