; RavenFabric NSIS Installer Script
; Build with: makensis deploy/nsis/ravenfabric.nsi
; Requires: NSIS 3.x (https://nsis.sourceforge.io/)

!define PRODUCT_NAME "RavenFabric"
!define PRODUCT_VERSION "0.5.0"
!define PRODUCT_PUBLISHER "Erling Kristiansen"
!define PRODUCT_WEB_SITE "https://ravenfabric.io"
!define PRODUCT_UNINST_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCT_NAME}"
!define PRODUCT_DIR_REGKEY "Software\RavenFabric"

; Installer attributes
Name "${PRODUCT_NAME} ${PRODUCT_VERSION}"
OutFile "ravenfabric-${PRODUCT_VERSION}-x64-setup.exe"
InstallDir "$PROGRAMFILES64\RavenFabric"
InstallDirRegKey HKLM "${PRODUCT_DIR_REGKEY}" ""
RequestExecutionLevel admin
Unicode True

; Compression
SetCompressor /SOLID lzma

; Modern UI
!include "MUI2.nsh"
!include "nsDialogs.nsh"

; MUI Settings
!define MUI_ABORTWARNING
!define MUI_ICON "${NSISDIR}\Contrib\Graphics\Icons\modern-install.ico"
!define MUI_UNICON "${NSISDIR}\Contrib\Graphics\Icons\modern-uninstall.ico"

; Pages
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "..\..\LICENSES\AGPLv3.txt"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_COMPONENTS
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

; Uninstaller pages
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

; Language
!insertmacro MUI_LANGUAGE "English"

; -----------------------------------------------
; Sections
; -----------------------------------------------

Section "Core Binaries (required)" SEC_CORE
  SectionIn RO

  SetOutPath "$INSTDIR\bin"
  File "..\..\target\release\rf.exe"
  File "..\..\target\release\rf-agent.exe"
  File "..\..\target\release\rf-relay.exe"
  File "..\..\target\release\rf-mcp-server.exe"

  ; Example configuration
  SetOutPath "$INSTDIR\config"
  File "..\raven.toml.example"

  ; Write registry keys
  WriteRegStr HKLM "${PRODUCT_DIR_REGKEY}" "InstallDir" "$INSTDIR"
  WriteRegStr HKLM "${PRODUCT_UNINST_KEY}" "DisplayName" "${PRODUCT_NAME}"
  WriteRegStr HKLM "${PRODUCT_UNINST_KEY}" "UninstallString" "$INSTDIR\uninstall.exe"
  WriteRegStr HKLM "${PRODUCT_UNINST_KEY}" "DisplayIcon" "$INSTDIR\bin\rf.exe"
  WriteRegStr HKLM "${PRODUCT_UNINST_KEY}" "DisplayVersion" "${PRODUCT_VERSION}"
  WriteRegStr HKLM "${PRODUCT_UNINST_KEY}" "Publisher" "${PRODUCT_PUBLISHER}"
  WriteRegStr HKLM "${PRODUCT_UNINST_KEY}" "URLInfoAbout" "${PRODUCT_WEB_SITE}"
  WriteRegDWORD HKLM "${PRODUCT_UNINST_KEY}" "NoModify" 1
  WriteRegDWORD HKLM "${PRODUCT_UNINST_KEY}" "NoRepair" 1

  ; Create uninstaller
  WriteUninstaller "$INSTDIR\uninstall.exe"
SectionEnd

Section "Add to PATH" SEC_PATH
  ; Add bin directory to system PATH
  EnVar::SetHKLM
  EnVar::AddValue "PATH" "$INSTDIR\bin"
SectionEnd

Section "Install Agent Service" SEC_SERVICE
  ; Install rf-agent as a Windows service
  nsExec::ExecToLog '"$INSTDIR\bin\rf-agent.exe" service install --config "$INSTDIR\config\raven.toml"'
  Pop $0
  ${If} $0 != 0
    ; Fallback: use sc.exe
    nsExec::ExecToLog 'sc.exe create RavenFabricAgent binPath= "\"$INSTDIR\bin\rf-agent.exe\" --config \"$INSTDIR\config\raven.toml\"" start= auto DisplayName= "RavenFabric Agent"'
    nsExec::ExecToLog 'sc.exe description RavenFabricAgent "Secure remote execution agent - zero-trust, cryptographically verified"'
  ${EndIf}
SectionEnd

Section "Start Menu Shortcuts" SEC_SHORTCUTS
  CreateDirectory "$SMPROGRAMS\RavenFabric"
  CreateShortcut "$SMPROGRAMS\RavenFabric\RavenFabric Website.lnk" "${PRODUCT_WEB_SITE}"
  CreateShortcut "$SMPROGRAMS\RavenFabric\Uninstall.lnk" "$INSTDIR\uninstall.exe"
SectionEnd

; -----------------------------------------------
; Section descriptions
; -----------------------------------------------

!insertmacro MUI_FUNCTION_DESCRIPTION_BEGIN
  !insertmacro MUI_DESCRIPTION_TEXT ${SEC_CORE} "Core binaries: rf (CLI), rf-agent, rf-relay, rf-mcp-server"
  !insertmacro MUI_DESCRIPTION_TEXT ${SEC_PATH} "Add RavenFabric to system PATH for command-line access"
  !insertmacro MUI_DESCRIPTION_TEXT ${SEC_SERVICE} "Install rf-agent as a Windows service (starts automatically)"
  !insertmacro MUI_DESCRIPTION_TEXT ${SEC_SHORTCUTS} "Create Start Menu shortcuts"
!insertmacro MUI_FUNCTION_DESCRIPTION_END

; -----------------------------------------------
; Uninstaller
; -----------------------------------------------

Section "Uninstall"
  ; Stop and remove service
  nsExec::ExecToLog 'sc.exe stop RavenFabricAgent'
  nsExec::ExecToLog 'sc.exe delete RavenFabricAgent'

  ; Remove from PATH
  EnVar::SetHKLM
  EnVar::DeleteValue "PATH" "$INSTDIR\bin"

  ; Remove files
  Delete "$INSTDIR\bin\rf.exe"
  Delete "$INSTDIR\bin\rf-agent.exe"
  Delete "$INSTDIR\bin\rf-relay.exe"
  Delete "$INSTDIR\bin\rf-mcp-server.exe"
  Delete "$INSTDIR\config\raven.toml.example"
  Delete "$INSTDIR\uninstall.exe"

  ; Remove directories (only if empty)
  RMDir "$INSTDIR\bin"
  RMDir "$INSTDIR\config"
  RMDir "$INSTDIR"

  ; Remove Start Menu
  Delete "$SMPROGRAMS\RavenFabric\RavenFabric Website.lnk"
  Delete "$SMPROGRAMS\RavenFabric\Uninstall.lnk"
  RMDir "$SMPROGRAMS\RavenFabric"

  ; Remove registry keys
  DeleteRegKey HKLM "${PRODUCT_UNINST_KEY}"
  DeleteRegKey HKLM "${PRODUCT_DIR_REGKEY}"
SectionEnd
