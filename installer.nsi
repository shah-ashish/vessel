; ====================================================================
; Vessel - NSIS Modern Windows Installer Script
; ====================================================================

!include "MUI2.nsh"

; General Settings
Name "Vessel - Laptop Audio Stream"
OutFile "Vessel-Setup-x64.exe"
InstallDir "$PROGRAMFILES64\Vessel"
InstallDirRegKey HKLM "Software\Vessel" "Install_Dir"
RequestExecutionLevel admin

; UI Configuration
!define MUI_ABORTWARNING
!define MUI_HEADERIMAGE
!define MUI_WELCOMEFINISHPAGE_BITMAP_NOSTRETCH

; Installer Pages
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES

; Finish Page: Option to launch immediately
!define MUI_FINISHPAGE_RUN "$INSTDIR\laptop-audio-stream.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Launch Vessel Laptop Audio Stream"
!insertmacro MUI_PAGE_FINISH

; Uninstaller Pages
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "English"

; --------------------------------------------------------------------
; Installation Section
; --------------------------------------------------------------------
Section "Vessel (required)" SecCore
  SectionIn RO

  ; Target all users for desktop and start menu shortcuts
  SetShellVarContext all

  SetOutPath "$INSTDIR"
  File "target\release\laptop-audio-stream.exe"

  ; Registry key for install location
  WriteRegStr HKLM "Software\Vessel" "Install_Dir" "$INSTDIR"

  ; Register with Windows Add/Remove Programs
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\Vessel" "DisplayName" "Vessel - Laptop Audio Stream"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\Vessel" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\Vessel" "DisplayIcon" '"$INSTDIR\laptop-audio-stream.exe"'
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\Vessel" "Publisher" "Vessel Audio"
  WriteRegDWORD HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\Vessel" "NoModify" 1
  WriteRegDWORD HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\Vessel" "NoRepair" 1

  ; Create uninstaller executable
  WriteUninstaller "$INSTDIR\uninstall.exe"

  ; Create Start Menu & Desktop Shortcuts
  CreateDirectory "$SMPROGRAMS\Vessel"
  CreateShortcut "$SMPROGRAMS\Vessel\Vessel.lnk" "$INSTDIR\laptop-audio-stream.exe" "" "$INSTDIR\laptop-audio-stream.exe" 0
  CreateShortcut "$SMPROGRAMS\Vessel\Uninstall Vessel.lnk" "$INSTDIR\uninstall.exe" "" "$INSTDIR\uninstall.exe" 0
  CreateShortcut "$DESKTOP\Vessel.lnk" "$INSTDIR\laptop-audio-stream.exe" "" "$INSTDIR\laptop-audio-stream.exe" 0
SectionEnd

; --------------------------------------------------------------------
; Uninstaller Section
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
