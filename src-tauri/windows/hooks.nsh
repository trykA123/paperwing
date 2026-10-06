!macro NSIS_HOOK_POSTINSTALL
  WriteRegStr HKCU "Software\Classes\Directory\shell\Skein" "" "Open with Skein"
  WriteRegStr HKCU "Software\Classes\Directory\shell\Skein" "Icon" "$\"$INSTDIR\${MAINBINARYNAME}.exe$\""
  WriteRegStr HKCU "Software\Classes\Directory\shell\Skein\command" "" "$\"$INSTDIR\${MAINBINARYNAME}.exe$\" $\"%1$\""
  WriteRegStr HKCU "Software\Classes\Directory\Background\shell\Skein" "" "Open with Skein"
  WriteRegStr HKCU "Software\Classes\Directory\Background\shell\Skein" "Icon" "$\"$INSTDIR\${MAINBINARYNAME}.exe$\""
  WriteRegStr HKCU "Software\Classes\Directory\Background\shell\Skein\command" "" "$\"$INSTDIR\${MAINBINARYNAME}.exe$\" $\"%V$\""
  WriteRegStr HKCU "Software\Classes\Drive\shell\Skein" "" "Open with Skein"
  WriteRegStr HKCU "Software\Classes\Drive\shell\Skein" "Icon" "$\"$INSTDIR\${MAINBINARYNAME}.exe$\""
  WriteRegStr HKCU "Software\Classes\Drive\shell\Skein\command" "" "$\"$INSTDIR\${MAINBINARYNAME}.exe$\" $\"%1$\""
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  DeleteRegKey HKCU "Software\Classes\Directory\shell\Skein"
  DeleteRegKey HKCU "Software\Classes\Directory\Background\shell\Skein"
  DeleteRegKey HKCU "Software\Classes\Drive\shell\Skein"
!macroend
