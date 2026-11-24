; Установщик Anvil для Windows (Inno Setup 6.7+).
;
; Сборка:  ISCC.exe /DAppVersion=0.2.0 installer\anvil.iss
; Результат: dist\anvil-setup-x64.exe
;
; По умолчанию ставится только для текущего пользователя — без прав
; администратора, в %LOCALAPPDATA%\Programs\Anvil. В мастере можно выбрать
; установку для всех пользователей (тогда Windows спросит разрешение).
;
; Обновление из самого Anvil: установщик запускается тихо в ту же папку —
;   anvil-setup-x64.exe /SILENT /DIR=… /CURRENTUSER /RELAUNCH=1
; закрывает работающий Anvil, ставит новую версию и открывает её с --updated.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef SourceExe
  #define SourceExe "..\target\release\anvil.exe"
#endif

#define AppName "Anvil"
#define AppExe "anvil.exe"
#define AppUrl "https://github.com/mylifeitsbest/anvil"

[Setup]
AppId={{B6F2D3A1-7C4E-4F8A-9E21-5D3C8A1F0E47}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher=Anvil contributors
AppPublisherURL={#AppUrl}
AppSupportURL={#AppUrl}/issues
AppUpdatesURL={#AppUrl}/releases
AppComments=Быстрая и лёгкая IDE на Rust
DefaultDirName={autopf}\{#AppName}
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
DisableWelcomePage=no
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=commandline dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
OutputDir=..\dist
OutputBaseFilename=anvil-setup-x64
SetupIconFile=..\assets\icon\anvil.ico
UninstallDisplayIcon={app}\{#AppExe}
UninstallDisplayName={#AppName}
LicenseFile=..\LICENSE
Compression=lzma2/ultra64
SolidCompression=yes
CloseApplications=yes
RestartApplications=no
ChangesEnvironment=yes
WizardStyle=modern dark hidebevels
WizardImageFile=wizard.png,wizard@2x.png
WizardSmallImageFile=small.png,small@2x.png
WizardImageBackColor=#0f0f12
VersionInfoVersion={#AppVersion}
VersionInfoProductName={#AppName}
VersionInfoDescription={#AppName} — установщик

[Languages]
Name: "russian"; MessagesFile: "compiler:Languages\Russian.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[CustomMessages]
russian.Integration=Интеграция с Windows:
english.Integration=Windows integration:
russian.FolderMenu=Добавить «Открыть в Anvil» в меню папок и файлов
english.FolderMenu=Add "Open in Anvil" to the folder and file context menu
russian.AddPath=Добавить anvil в PATH (запуск «anvil .» из терминала)
english.AddPath=Add anvil to PATH (run "anvil ." from a terminal)
russian.OpenIn=Открыть в Anvil
english.OpenIn=Open in Anvil
russian.Links=Открывать ссылки-приглашения anvil:// (совместная работа)
english.Links=Open anvil:// invite links (collaboration)

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"
Name: "contextmenu"; Description: "{cm:FolderMenu}"; GroupDescription: "{cm:Integration}"
Name: "addtopath"; Description: "{cm:AddPath}"; GroupDescription: "{cm:Integration}"; Flags: unchecked
Name: "links"; Description: "{cm:Links}"; GroupDescription: "{cm:Integration}"

[Files]
Source: "{#SourceExe}"; DestDir: "{app}"; DestName: "{#AppExe}"; Flags: ignoreversion
Source: "..\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; DestName: "LICENSE.txt"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\{#AppExe}"; WorkingDir: "{userdocs}"; Comment: "Быстрая и лёгкая IDE на Rust"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExe}"; WorkingDir: "{userdocs}"; Tasks: desktopicon

[Registry]
; «Открыть в Anvil» для папки, фона папки и любого файла
Root: HKA; Subkey: "Software\Classes\Directory\shell\Anvil"; ValueType: string; ValueName: ""; ValueData: "{cm:OpenIn}"; Tasks: contextmenu; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Directory\shell\Anvil"; ValueType: string; ValueName: "Icon"; ValueData: """{app}\{#AppExe}"""; Tasks: contextmenu
Root: HKA; Subkey: "Software\Classes\Directory\shell\Anvil\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""; Tasks: contextmenu
Root: HKA; Subkey: "Software\Classes\Directory\Background\shell\Anvil"; ValueType: string; ValueName: ""; ValueData: "{cm:OpenIn}"; Tasks: contextmenu; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Directory\Background\shell\Anvil"; ValueType: string; ValueName: "Icon"; ValueData: """{app}\{#AppExe}"""; Tasks: contextmenu
Root: HKA; Subkey: "Software\Classes\Directory\Background\shell\Anvil\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%V"""; Tasks: contextmenu
Root: HKA; Subkey: "Software\Classes\*\shell\Anvil"; ValueType: string; ValueName: ""; ValueData: "{cm:OpenIn}"; Tasks: contextmenu; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\*\shell\Anvil"; ValueType: string; ValueName: "Icon"; ValueData: """{app}\{#AppExe}"""; Tasks: contextmenu
Root: HKA; Subkey: "Software\Classes\*\shell\Anvil\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""; Tasks: contextmenu
; ссылки-приглашения anvil:// → «anvil.exe --join ССЫЛКА» (уже открытое окно получит ссылку само)
Root: HKA; Subkey: "Software\Classes\anvil"; ValueType: string; ValueName: ""; ValueData: "URL:Anvil"; Tasks: links; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\anvil"; ValueType: string; ValueName: "URL Protocol"; ValueData: ""; Tasks: links
Root: HKA; Subkey: "Software\Classes\anvil\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"",0"; Tasks: links
Root: HKA; Subkey: "Software\Classes\anvil\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" --join ""%1"""; Tasks: links
; PATH пользователя
Root: HKCU; Subkey: "Environment"; ValueType: expandsz; ValueName: "Path"; ValueData: "{olddata};{app}"; Tasks: addtopath; Check: NeedsAddPath(ExpandConstant('{app}'))

[Run]
Filename: "{app}\{#AppExe}"; Description: "{cm:LaunchProgram,{#AppName}}"; Flags: nowait postinstall skipifsilent
; обновление из Anvil: открыть новую версию (прошлая сессия восстановится)
Filename: "{app}\{#AppExe}"; Parameters: "--updated"; WorkingDir: "{userdocs}"; Flags: nowait runasoriginaluser skipifnotsilent; Check: RelaunchRequested

[Code]
function RelaunchRequested: Boolean;
begin
  Result := ExpandConstant('{param:RELAUNCH|0}') = '1';
end;

function NeedsAddPath(Dir: string): Boolean;
var
  Path: string;
begin
  if not RegQueryStringValue(HKEY_CURRENT_USER, 'Environment', 'Path', Path) then
  begin
    Result := True;
    exit;
  end;
  Result := Pos(';' + Uppercase(Dir) + ';', ';' + Uppercase(Path) + ';') = 0;
end;

procedure RemoveFromPath(Dir: string);
var
  Path: string;
  P: Integer;
begin
  if not RegQueryStringValue(HKEY_CURRENT_USER, 'Environment', 'Path', Path) then
    exit;
  P := Pos(';' + Uppercase(Dir), Uppercase(Path));
  if P > 0 then
  begin
    Delete(Path, P, Length(Dir) + 1);
    RegWriteExpandStringValue(HKEY_CURRENT_USER, 'Environment', 'Path', Path);
  end;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  { настройки пользователя (%APPDATA%\Anvil) не трогаем — они переживут переустановку }
  if CurUninstallStep = usPostUninstall then
    RemoveFromPath(ExpandConstant('{app}'));
end;
