; ====================================================================
; Vessel - Windows installer
; ====================================================================

!include "MUI2.nsh"

Name "Vessel"
OutFile "Vessel-Setup-x64.exe"
InstallDir "$PROGRAMFILES64\Vessel"
InstallDirRegKey HKLM "Software\Vessel" "Install_Dir"
RequestExecutionLevel admin

!define MUI_ABORTWARNING
!define MUI_ICON "assets\vessel.ico"
!define MUI_UNICON "assets\vessel.ico"
!define MUI_HEADERIMAGE
!define MUI_WELCOMEFINISHPAGE_BITMAP_NOSTRETCH

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
; Ask the user whether a desktop shortcut should be created.
!insertmacro MUI_PAGE_COMPONENTS
!insertmacro MUI_PAGE_INSTFILES

!define MUI_FINISHPAGE_RUN "$INSTDIR\laptop-audio-stream.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Launch Vessel"
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "English"

; --------------------------------------------------------------------
; Required application files and Start Menu shortcuts
; --------------------------------------------------------------------
Section "Vessel (required)" SecCore
  SectionIn RO
  SetShellVarContext all

  SetOutPath "$INSTDIR"
  File "target\release\laptop-audio-stream.exe"
  SetOutPath "$INSTDIR\assets"
  File "assets\vessel.ico"
  SetOutPath "$INSTDIR"

  WriteRegStr HKLM "Software\Vessel" "Install_Dir" "$INSTDIR"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\Vessel" "DisplayName" "Vessel"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\Vessel" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\Vessel" "DisplayIcon" '"$INSTDIR\assets\vessel.ico"'
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\Vessel" "Publisher" "Vessel"
  WriteRegDWORD HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\Vessel" "NoModify" 1
  WriteRegDWORD HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\Vessel" "NoRepair" 1

  WriteUninstaller "$INSTDIR\uninstall.exe"

  CreateDirectory "$SMPROGRAMS\Vessel"
  CreateShortcut "$SMPROGRAMS\Vessel\Vessel.lnk" "$INSTDIR\laptop-audio-stream.exe" "" "$INSTDIR\laptop-audio-stream.exe" 0
  CreateShortcut "$SMPROGRAMS\Vessel\Uninstall Vessel.lnk" "$INSTDIR\uninstall.exe" "" "$INSTDIR\uninstall.exe" 0
SectionEnd

; --------------------------------------------------------------------
; Optional desktop shortcut. The user makes this choice during setup.
; --------------------------------------------------------------------
Section /o "Create a desktop shortcut" SecDesktopShortcut
  SetShellVarContext all
  CreateShortcut "$DESKTOP\Vessel.lnk" "$INSTDIR\laptop-audio-stream.exe" "" "$INSTDIR\laptop-audio-stream.exe" 0
SectionEnd

; --------------------------------------------------------------------
; Uninstaller
; --------------------------------------------------------------------
Section "Uninstall"
  SetShellVarContext all

  Delete "$DESKTOP\Vessel.lnk"
  Delete "$SMPROGRAMS\Vessel\Vessel.lnk"
  Delete "$SMPROGRAMS\Vessel\Uninstall Vessel.lnk"
  RMDir "$SMPROGRAMS\Vessel"

  Delete "$INSTDIR\laptop-audio-stream.exe"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"

  DeleteRegKey HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\Vessel"
  DeleteRegKey HKLM "Software\Vessel"
SectionEnd
