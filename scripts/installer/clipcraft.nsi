Unicode true
!include "MUI2.nsh"
!include "LogicLib.nsh"
!include "x64.nsh"
Name "ClipCraft ${VERSION}"
OutFile "${OUTPUT}"
InstallDir "$LOCALAPPDATA\Programs\ClipCraft"
RequestExecutionLevel user
SetCompressor /SOLID zlib
SetOverwrite on
ManifestDPIAware true
VIProductVersion "${BASE_VERSION}.0"
VIAddVersionKey "ProductName" "ClipCraft"
VIAddVersionKey "FileDescription" "ClipCraft per-user setup"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "LegalCopyright" "ClipCraft contributors"
!define KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\ClipCraft"
!define MUI_ICON "${REPO}\apps\desktop\assets\brand\clipcraft.ico"
!define MUI_UNICON "${REPO}\apps\desktop\assets\brand\clipcraft.ico"
!define MUI_ABORTWARNING
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "${REPO}\LICENSE"
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

!macro CheckWritable FILE
  ${If} ${FileExists} "$INSTDIR\${FILE}"
    ClearErrors
    FileOpen $0 "$INSTDIR\${FILE}" a
    ${If} ${Errors}
      MessageBox MB_OK|MB_ICONSTOP "Exit ClipCraft from its system tray menu, then retry setup or uninstall." /SD IDOK
      SetErrorLevel 2
      Quit
    ${EndIf}
    FileClose $0
  ${EndIf}
!macroend

Function .onInit
  ${IfNot} ${RunningX64}
    MessageBox MB_OK|MB_ICONSTOP "ClipCraft requires 64-bit Windows." /SD IDOK
    SetErrorLevel 3
    Quit
  ${EndIf}
  SetShellVarContext current
  SetRegView 64
  ; Fixed per-user location keeps upgrades and removal confined to this app.
  StrCpy $INSTDIR "$LOCALAPPDATA\Programs\ClipCraft"
  !insertmacro CheckWritable "ClipCraft.exe"
  !insertmacro CheckWritable "capture-cli.exe"
FunctionEnd

Section "ClipCraft"
  SetOutPath "$INSTDIR"
  !include "${INSTALL_FILES}"
  WriteUninstaller "$INSTDIR\Uninstall.exe"
  CreateShortCut "$SMPROGRAMS\ClipCraft Editor.lnk" "$INSTDIR\ClipCraft.exe" "--editor" "$INSTDIR\ClipCraft.exe" -101
  CreateShortCut "$SMPROGRAMS\ClipCraft Capture Controller.lnk" "$INSTDIR\ClipCraft.exe" "--capture-controller" "$INSTDIR\ClipCraft.exe" -102
  WriteRegStr HKCU "${KEY}" "DisplayName" "ClipCraft"
  WriteRegStr HKCU "${KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${KEY}" "Publisher" "ClipCraft contributors"
  WriteRegStr HKCU "${KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${KEY}" "DisplayIcon" "$INSTDIR\ClipCraft.exe,0"
  WriteRegStr HKCU "${KEY}" "UninstallString" '$\"$INSTDIR\Uninstall.exe$\"'
  WriteRegStr HKCU "${KEY}" "QuietUninstallString" '$\"$INSTDIR\Uninstall.exe$\" /S'
  WriteRegStr HKCU "${KEY}" "URLInfoAbout" "https://github.com/pashiadesigns/ClipCraft"
  WriteRegDWORD HKCU "${KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${KEY}" "NoRepair" 1
  WriteRegDWORD HKCU "${KEY}" "EstimatedSize" ${SIZE_KB}
SectionEnd

Function un.onInit
  SetShellVarContext current
  SetRegView 64
  StrCmp $INSTDIR "$LOCALAPPDATA\Programs\ClipCraft" +4
  MessageBox MB_OK|MB_ICONSTOP "Run the uninstaller from the original ClipCraft installation folder." /SD IDOK
  SetErrorLevel 4
  Quit
  !insertmacro CheckWritable "ClipCraft.exe"
  !insertmacro CheckWritable "capture-cli.exe"
FunctionEnd

Section "Uninstall"
  ; Explicit owned files only: never recursively remove a user-selected folder.
  !include "${REMOVE_FILES}"
  Delete "$SMPROGRAMS\ClipCraft Editor.lnk"
  Delete "$SMPROGRAMS\ClipCraft Capture Controller.lnk"
  DeleteRegKey HKCU "${KEY}"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir "$INSTDIR"
SectionEnd
