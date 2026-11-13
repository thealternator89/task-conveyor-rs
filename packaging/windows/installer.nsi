; Task Conveyor Windows Installer Script (NSIS)
; One-click per-user installation matching Squirrel / modern Electron behavior.

!include "MUI2.nsh"
!include "FileFunc.nsh"

; --------------------------------------------------
; General Definitions
; --------------------------------------------------
!define PRODUCT_NAME "Task Conveyor"
!define PRODUCT_PUBLISHER "Task Conveyor"
!define APP_DIR_NAME "TaskConveyor"
!define EXECUTABLE_NAME "task-conveyor-rs.exe"

!ifndef VERSION
  !define VERSION "0.1.0"
!endif

; Resolve icon path whether makensis runs with /NOCD from root or from script dir
!ifndef ICON_PATH
  !ifFileExists "assets\icon.ico"
    !define ICON_PATH "assets\icon.ico"
  !else
    !define ICON_PATH "..\..\assets\icon.ico"
  !endif
!endif

; Resolve executable path whether makensis runs with /NOCD from root or from script dir
!ifndef EXE_PATH
  !ifFileExists "target\x86_64-pc-windows-msvc\release\task-conveyor-rs.exe"
    !define EXE_PATH "target\x86_64-pc-windows-msvc\release\task-conveyor-rs.exe"
  !else
    !define EXE_PATH "..\..\target\x86_64-pc-windows-msvc\release\task-conveyor-rs.exe"
  !endif
!endif

!ifndef OUTPUT_FILE
  !define OUTPUT_FILE "target\x86_64-pc-windows-msvc\release\TaskConveyor-Setup.exe"
!endif

; --------------------------------------------------
; Installer Settings
; --------------------------------------------------
Name "${PRODUCT_NAME}"
OutFile "${OUTPUT_FILE}"
InstallDir "$LOCALAPPDATA\Programs\${APP_DIR_NAME}"
RequestExecutionLevel user
SetCompressor /SOLID lzma

; Icons
!define MUI_ICON "${ICON_PATH}"
!define MUI_UNICON "${ICON_PATH}"

; Version Information
VIProductVersion "${VERSION}.0"
VIAddVersionKey "ProductName" "${PRODUCT_NAME}"
VIAddVersionKey "CompanyName" "${PRODUCT_PUBLISHER}"
VIAddVersionKey "LegalCopyright" "${PRODUCT_PUBLISHER}"
VIAddVersionKey "FileDescription" "${PRODUCT_NAME} Setup"
VIAddVersionKey "FileVersion" "${VERSION}.0"
VIAddVersionKey "ProductVersion" "${VERSION}"

; --------------------------------------------------
; Modern UI Settings (One-Click / Minimal UI)
; --------------------------------------------------
!define MUI_ABORTWARNING
AutoCloseWindow true
ShowInstDetails nevershow

!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

; --------------------------------------------------
; Installer Section
; --------------------------------------------------
Section "MainSection" SEC01
  ; Terminate running instance if any to allow replacing binary cleanly
  nsExec::Exec 'taskkill /F /IM ${EXECUTABLE_NAME}'

  ; Set output destination and copy executable and icon
  SetOutPath "$INSTDIR"
  File "/oname=${EXECUTABLE_NAME}" "${EXE_PATH}"
  File "/oname=icon.ico" "${ICON_PATH}"

  ; Write uninstaller
  WriteUninstaller "$INSTDIR\uninstall.exe"

  ; Create Start Menu shortcut (No desktop shortcut as configured)
  CreateDirectory "$SMPROGRAMS\${PRODUCT_NAME}"
  CreateShortcut "$SMPROGRAMS\${PRODUCT_NAME}\${PRODUCT_NAME}.lnk" "$INSTDIR\${EXECUTABLE_NAME}" "" "$INSTDIR\icon.ico" 0

  ; Register in Windows Add/Remove Programs (HKCU per-user)
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_DIR_NAME}" "DisplayName" "${PRODUCT_NAME}"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_DIR_NAME}" "DisplayIcon" "$INSTDIR\icon.ico,0"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_DIR_NAME}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_DIR_NAME}" "Publisher" "${PRODUCT_PUBLISHER}"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_DIR_NAME}" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_DIR_NAME}" "QuietUninstallString" '"$INSTDIR\uninstall.exe" /S'
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_DIR_NAME}" "InstallLocation" "$INSTDIR"
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_DIR_NAME}" "NoModify" 1
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_DIR_NAME}" "NoRepair" 1

  ; Immediately launch the installed application
  Exec '"$INSTDIR\${EXECUTABLE_NAME}"'
SectionEnd

; --------------------------------------------------
; Uninstaller Section
; --------------------------------------------------
Section "Uninstall"
  ; Terminate running instance before deleting files
  nsExec::Exec 'taskkill /F /IM ${EXECUTABLE_NAME}'

  ; Remove Start Menu shortcut and folder
  Delete "$SMPROGRAMS\${PRODUCT_NAME}\${PRODUCT_NAME}.lnk"
  RMDir "$SMPROGRAMS\${PRODUCT_NAME}"

  ; Remove application files
  Delete "$INSTDIR\${EXECUTABLE_NAME}"
  Delete "$INSTDIR\icon.ico"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"

  ; Remove Add/Remove Programs registry key
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_DIR_NAME}"
SectionEnd
