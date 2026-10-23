; The Windows installer, built by scripts/package-experimental.sh:
;   iscc -DVersion=0.9.0 -DArch=x86_64 -DSource=dist\riptide-0.9.0-windows-x86_64 -DOutput=dist riptide.iss
; It installs for the current user without admin rights, or for everyone when
; run as admin, and can list riptide in Windows' default apps.

#ifndef Version
  #error Pass -DVersion=<version>
#endif
#ifndef Source
  #error Pass -DSource=<folder with riptide.exe and the CEF runtime>
#endif
#ifndef Output
  #define Output "."
#endif
#ifndef Arch
  #define Arch "x86_64"
#endif
#if Arch == "aarch64"
  #define WindowsArch "arm64"
#else
  #define WindowsArch "x64compatible"
#endif

[Setup]
AppId={{6F1C2E4A-8B3D-4E5F-9A7C-2D1B0E3F4A5B}
AppName=riptide
AppVersion={#Version}
AppVerName=riptide {#Version}
AppPublisher=riptide
AppPublisherURL=https://github.com/joshzcold/riptide
AppSupportURL=https://github.com/joshzcold/riptide/issues
DefaultDirName={autopf}\riptide
DefaultGroupName=riptide
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog commandline
ArchitecturesAllowed={#WindowsArch}
ArchitecturesInstallIn64BitMode={#WindowsArch}
LicenseFile={#Source}\LICENSE
SetupIconFile=riptide.ico
UninstallDisplayIcon={app}\riptide.ico
OutputDir={#Output}
OutputBaseFilename=riptide-{#Version}-windows-{#Arch}-setup
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
ChangesAssociations=yes

[Tasks]
Name: desktopicon; Description: "Create a desktop shortcut"; Flags: unchecked
Name: browser; Description: "List riptide in Windows' default apps, for web links and HTML files"

[Files]
Source: "{#Source}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "riptide.ico"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\riptide"; Filename: "{app}\riptide.exe"; IconFilename: "{app}\riptide.ico"
Name: "{autodesktop}\riptide"; Filename: "{app}\riptide.exe"; IconFilename: "{app}\riptide.ico"; Tasks: desktopicon

[Registry]
; Windows' default apps list: a browser registers its capabilities and the
; classes they point to, and the user picks it in Settings.
Root: HKA; Subkey: "Software\Classes\riptideURL"; ValueType: string; ValueData: "riptide URL"; Flags: uninsdeletekey; Tasks: browser
Root: HKA; Subkey: "Software\Classes\riptideURL"; ValueType: string; ValueName: "URL Protocol"; ValueData: ""; Tasks: browser
Root: HKA; Subkey: "Software\Classes\riptideURL\DefaultIcon"; ValueType: string; ValueData: "{app}\riptide.ico"; Tasks: browser
Root: HKA; Subkey: "Software\Classes\riptideURL\shell\open\command"; ValueType: string; ValueData: """{app}\riptide.exe"" ""%1"""; Tasks: browser
Root: HKA; Subkey: "Software\Classes\riptideHTML"; ValueType: string; ValueData: "riptide HTML document"; Flags: uninsdeletekey; Tasks: browser
Root: HKA; Subkey: "Software\Classes\riptideHTML\DefaultIcon"; ValueType: string; ValueData: "{app}\riptide.ico"; Tasks: browser
Root: HKA; Subkey: "Software\Classes\riptideHTML\shell\open\command"; ValueType: string; ValueData: """{app}\riptide.exe"" ""%1"""; Tasks: browser
Root: HKA; Subkey: "Software\Clients\StartMenuInternet\riptide"; ValueType: string; ValueData: "riptide"; Flags: uninsdeletekey; Tasks: browser
Root: HKA; Subkey: "Software\Clients\StartMenuInternet\riptide\DefaultIcon"; ValueType: string; ValueData: "{app}\riptide.ico"; Tasks: browser
Root: HKA; Subkey: "Software\Clients\StartMenuInternet\riptide\shell\open\command"; ValueType: string; ValueData: """{app}\riptide.exe"""; Tasks: browser
Root: HKA; Subkey: "Software\Clients\StartMenuInternet\riptide\Capabilities"; ValueType: string; ValueName: "ApplicationName"; ValueData: "riptide"; Tasks: browser
Root: HKA; Subkey: "Software\Clients\StartMenuInternet\riptide\Capabilities"; ValueType: string; ValueName: "ApplicationDescription"; ValueData: "A keyboard-driven browser"; Tasks: browser
Root: HKA; Subkey: "Software\Clients\StartMenuInternet\riptide\Capabilities"; ValueType: string; ValueName: "ApplicationIcon"; ValueData: "{app}\riptide.ico"; Tasks: browser
Root: HKA; Subkey: "Software\Clients\StartMenuInternet\riptide\Capabilities\StartMenu"; ValueType: string; ValueName: "StartMenuInternet"; ValueData: "riptide"; Tasks: browser
Root: HKA; Subkey: "Software\Clients\StartMenuInternet\riptide\Capabilities\URLAssociations"; ValueType: string; ValueName: "http"; ValueData: "riptideURL"; Tasks: browser
Root: HKA; Subkey: "Software\Clients\StartMenuInternet\riptide\Capabilities\URLAssociations"; ValueType: string; ValueName: "https"; ValueData: "riptideURL"; Tasks: browser
Root: HKA; Subkey: "Software\Clients\StartMenuInternet\riptide\Capabilities\FileAssociations"; ValueType: string; ValueName: ".html"; ValueData: "riptideHTML"; Tasks: browser
Root: HKA; Subkey: "Software\Clients\StartMenuInternet\riptide\Capabilities\FileAssociations"; ValueType: string; ValueName: ".htm"; ValueData: "riptideHTML"; Tasks: browser
Root: HKA; Subkey: "Software\RegisteredApplications"; ValueType: string; ValueName: "riptide"; ValueData: "Software\Clients\StartMenuInternet\riptide\Capabilities"; Flags: uninsdeletevalue; Tasks: browser

[Run]
Filename: "{app}\riptide.exe"; Description: "Start riptide"; Flags: nowait postinstall skipifsilent
