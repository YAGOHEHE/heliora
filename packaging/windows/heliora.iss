; Instalador de Heliora para Windows (Inno Setup 6).
; Instala por usuario, sin permisos de administrador: el motor escribe cachés y entornos junto a sí
; mismo, y esa carpeta tiene que ser escribible.
; Uso: ISCC.exe /DVersion=1.2.0 /DOrigen=..\..\dist\Heliora /DSalida=..\..\dist heliora.iss

#ifndef Version
  #define Version "0.0.0"
#endif
#ifndef Origen
  #define Origen "..\..\dist\Heliora"
#endif
#ifndef Salida
  #define Salida "..\..\dist"
#endif

[Setup]
AppId={{6B0E1C52-6C1D-4E43-9B7B-4E1F2C8D0A11}
AppName=Heliora
AppVersion={#Version}
AppPublisher=Heliora
DefaultDirName={localappdata}\Programs\Heliora
DefaultGroupName=Heliora
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir={#Salida}
OutputBaseFilename=Heliora-Setup-{#Version}
SetupIconFile=..\..\icon\heliora.ico
UninstallDisplayIcon={app}\heliora.exe
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern

[Languages]
Name: "es"; MessagesFile: "compiler:Languages\Spanish.isl"

[Tasks]
Name: "escritorio"; Description: "Crear un acceso directo en el Escritorio"; GroupDescription: "Accesos directos:"

[Files]
Source: "{#Origen}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\Heliora"; Filename: "{app}\heliora.exe"; WorkingDir: "{app}"
Name: "{autodesktop}\Heliora"; Filename: "{app}\heliora.exe"; WorkingDir: "{app}"; Tasks: escritorio

[Run]
Filename: "{app}\heliora.exe"; Description: "Abrir Heliora"; Flags: nowait postinstall skipifsilent

[UninstallDelete]
; Cachés de Python que el motor genera al ejecutarse; no las instaló el instalador
Type: filesandordirs; Name: "{app}\engine"
Type: filesandordirs; Name: "{app}\python"
