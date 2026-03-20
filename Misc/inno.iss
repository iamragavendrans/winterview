#define MyAppName "Winterview Enhanced"
#define MyAppVersion "2.1.0"
#define MyAppPublisher "Radiantly (enhanced fork)"
#define MyAppURL "https://github.com/radiantly/Winterview"
#define MyAppExeName "Winterview.exe"

[Setup]
AppId={{A7F82C1E-3D56-4E89-B012-8C3D7A94F501}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}
AppUpdatesURL={#MyAppURL}
DefaultDirName={autopf}\{#MyAppName}
UninstallDisplayIcon={app}\{#MyAppExeName}
; Require Windows 10 v2004+ (SetWindowDisplayAffinity WDA_EXCLUDEFROMCAPTURE)
MinVersion=10.0.19041
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
DisableProgramGroupPage=yes
PrivilegesRequiredOverridesAllowed=dialog
OutputBaseFilename=WinterviewEnhancedInstaller
OutputDir=Output
WizardStyle=modern
; Allow running without UAC prompt if user chooses
PrivilegesRequired=lowest

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon";    Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked
Name: "startupentry";   Description: "Start Winterview when Windows starts"; GroupDescription: "Startup:"; Flags: unchecked

[Files]
; Main executable
Source: "..\dist\Winterview.exe"; DestDir: "{app}"; Flags: ignoreversion

; 64-bit payload DLL (injected into target processes)
Source: "..\dist\utils.dll";      DestDir: "{app}"; Flags: ignoreversion

; 32-bit payload DLL (injected into 32-bit target processes)
Source: "..\dist\utils32.dll";    DestDir: "{app}"; Flags: ignoreversion

; AutoHotkey helper script (optional hotkey alternative)
Source: "..\hide.ahk";            DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"
Name: "{autodesktop}\{#MyAppName}";  Filename: "{app}\{#MyAppExeName}"; Tasks: desktopicon

[Registry]
; Run on startup (only installed if user ticks the task checkbox)
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; \
  ValueType: string; ValueName: "WinterviewEnhanced"; \
  ValueData: """{app}\{#MyAppExeName}"""; \
  Flags: uninsdeletevalue; Tasks: startupentry

[Run]
Filename: "{app}\{#MyAppExeName}"; \
  Description: "{cm:LaunchProgram,{#StringChange(MyAppName, '&', '&&')}}"; \
  Flags: nowait postinstall skipifsilent

[UninstallRun]
; Gracefully exit the running instance before uninstall removes files
Filename: "taskkill.exe"; Parameters: "/F /IM {#MyAppExeName}"; Flags: runhidden; RunOnceId: "KillWinterview"

[Messages]
WelcomeLabel2=This will install [name/ver] on your computer.%n%nWinterview Enhanced hides windows from screen capture (e.g. during Zoom or Teams calls) while keeping them fully usable.%n%nNote: your antivirus may flag this as a false positive due to DLL injection. This is expected.
