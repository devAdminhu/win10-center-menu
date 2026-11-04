import json
import os
from pathlib import Path

root=Path(__file__).resolve().parent
apps=json.loads((root/'apps.json').read_text(encoding='utf-8-sig'))
names=['Google Chrome','Microsoft Edge','Explorador de Arquivos','Terminal','WhatsApp','Telegram','Claude','Blender 5.2','Fotos','Calculadora','Paint','Configurações','Roblox Studio','Runa','7-Zip File Manager','Bloco de Notas','Captura e Esboço','Microsoft Store']
indexed={a['Name']:a for a in apps}
ordered=[indexed[n] for n in names if n in indexed]
ordered += [a for a in apps if a['Name'] not in names]
lines=[]
shortcuts={}
for base in [os.environ.get('APPDATA',''),os.environ.get('PROGRAMDATA','')]:
    folder=Path(base)/'Microsoft/Windows/Start Menu/Programs'
    if folder.exists():
        for path in folder.rglob('*.lnk'):
            shortcuts.setdefault(path.stem.casefold(),str(path))
fallback={'Explorador de Arquivos':'C:/Windows/explorer.exe','Paint':'C:/Windows/System32/mspaint.exe','Bloco de Notas':'C:/Windows/System32/notepad.exe'}
for a in ordered:
    icon=a.get('IconPath') or shortcuts.get(a['Name'].casefold()) or fallback.get(a['Name'],'')
    lines.append('\t'.join(str(v or '').replace('\t',' ').replace('\r',' ').replace('\n',' ') for v in [a['Name'],a['AppID'],icon]))
(root/'apps.tsv').write_text('\n'.join(lines),encoding='utf-8')
print(len(lines),'aplicativos exportados; favoritos primeiro.')
