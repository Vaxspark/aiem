#define AppVersion GetEnv("AIEM_VERSION")
#define SourceDir GetEnv("AIEM_PACKAGE_DIR")
#define OutputDir GetEnv("AIEM_OUTPUT_DIR")
#define OutputBase GetEnv("AIEM_OUTPUT_BASE")

[Setup]
AppId={{D86E3D3A-6F1B-4F36-8B9B-0C47ED3E2E1C}
AppName=aiem
AppVersion={#AppVersion}
AppPublisher=aiem contributors
AppPublisherURL=https://github.com/Vaxspark/aiem
AppSupportURL=https://github.com/Vaxspark/aiem/issues
AppUpdatesURL=https://github.com/Vaxspark/aiem/releases
DefaultDirName={localappdata}\Programs\aiem
DefaultGroupName=aiem
DisableDirPage=no
PrivilegesRequired=lowest
OutputDir={#OutputDir}
OutputBaseFilename={#OutputBase}
SetupIconFile={#SourceDir}\aiem.ico
UninstallDisplayIcon={app}\aiem-gui.exe
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"
Name: "addtopath"; Description: "Add aiem to the user PATH"; GroupDescription: "Command line"

[Files]
Source: "{#SourceDir}\aiem.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\aiem-gui.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\aiem.ico"; DestDir: "{app}"; Flags: ignoreversion skipifsourcedoesntexist
Source: "{#SourceDir}\README.md"; DestDir: "{app}"; Flags: ignoreversion skipifsourcedoesntexist
Source: "{#SourceDir}\LICENSE"; DestDir: "{app}"; Flags: ignoreversion skipifsourcedoesntexist
Source: "{#SourceDir}\docs\*"; DestDir: "{app}\docs"; Flags: ignoreversion recursesubdirs createallsubdirs skipifsourcedoesntexist
Source: "{#SourceDir}\pic\*"; DestDir: "{app}\pic"; Flags: ignoreversion recursesubdirs createallsubdirs skipifsourcedoesntexist
Source: "{#SourceDir}\*.cmd"; DestDir: "{app}"; Flags: ignoreversion skipifsourcedoesntexist

[Icons]
Name: "{group}\aiem"; Filename: "{app}\aiem-gui.exe"; IconFilename: "{app}\aiem.ico"
Name: "{group}\aiem Web UI"; Filename: "{app}\aiem.exe"; Parameters: "serve --host 127.0.0.1 --port 8787 --open"; IconFilename: "{app}\aiem.ico"
Name: "{group}\Uninstall aiem"; Filename: "{uninstallexe}"
Name: "{autodesktop}\aiem"; Filename: "{app}\aiem-gui.exe"; IconFilename: "{app}\aiem.ico"; Tasks: desktopicon

[Run]
Filename: "{app}\aiem-gui.exe"; Description: "{cm:LaunchProgram,aiem}"; Flags: nowait postinstall skipifsilent unchecked

[Code]
const
  EnvironmentKey = 'Environment';

function PathContains(AppDir: string; PathValue: string): Boolean;
begin
  Result := Pos(';' + Uppercase(AppDir) + ';', ';' + Uppercase(PathValue) + ';') > 0;
end;

procedure AddUserPath(AppDir: string);
var
  CurrentPath: string;
begin
  if not RegQueryStringValue(HKEY_CURRENT_USER, EnvironmentKey, 'Path', CurrentPath) then begin
    CurrentPath := '';
  end;

  if not PathContains(AppDir, CurrentPath) then begin
    if CurrentPath = '' then begin
      RegWriteStringValue(HKEY_CURRENT_USER, EnvironmentKey, 'Path', AppDir);
    end else begin
      RegWriteStringValue(HKEY_CURRENT_USER, EnvironmentKey, 'Path', CurrentPath + ';' + AppDir);
    end;
  end;
end;

procedure RemoveUserPath(AppDir: string);
var
  CurrentPath: string;
begin
  if RegQueryStringValue(HKEY_CURRENT_USER, EnvironmentKey, 'Path', CurrentPath) then begin
    StringChangeEx(CurrentPath, AppDir + ';', '', True);
    StringChangeEx(CurrentPath, ';' + AppDir, '', True);
    if Uppercase(CurrentPath) = Uppercase(AppDir) then begin
      CurrentPath := '';
    end;
    RegWriteStringValue(HKEY_CURRENT_USER, EnvironmentKey, 'Path', CurrentPath);
  end;
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if (CurStep = ssPostInstall) and WizardIsTaskSelected('addtopath') then begin
    AddUserPath(ExpandConstant('{app}'));
  end;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usPostUninstall then begin
    RemoveUserPath(ExpandConstant('{app}'));
  end;
end;
