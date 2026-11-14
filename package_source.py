"""Create an audited source archive; no saves, compiled programs, caches or logs."""
import argparse
from pathlib import Path
import re
from tempfile import NamedTemporaryFile
from zipfile import ZIP_DEFLATED, ZipFile, ZipInfo

ROOT=Path(__file__).resolve().parent
DEFAULT_ARCHIVE=ROOT.parent / 'cat-home-source.zip'
ROOT_FILES=('.gitignore','.gitattributes','Cargo.toml','Cargo.lock','build.rs','README.md','LICENSE','CREDITS.md','package_source.py')
GROUPS={'src':{'.rs'},'examples':{'.rs'},'tools':{'.py'},'design':{'.tsv','.txt','.py','.md'},
        'docs':{'.md','.tsv','.json','.png'},'assets':{'.bgra','.json','.tiles','.tsv','.ico','.rc','.txt','.png'},
        'content-packs':{'.catpack','.json','.txt'},'sources':{'.png','.json','.zip'}}
INCLUDE=re.compile(r'include_(?:bytes|str)!\(\s*"([^"]+)"\s*\)')
LOCAL_PATH=re.compile(r'#\[path\s*=\s*"([^"]+)"\]')


def source_files():
    paths={}
    def add(path):
        resolved=path.resolve(strict=True)
        if resolved!=path or not path.is_file():
            raise ValueError(f'Not a regular local file: {path}')
        name=path.relative_to(ROOT).as_posix()
        if any(part.startswith('.') for part in path.relative_to(ROOT).parts) and name not in ('.gitignore','.gitattributes','.github/workflows/release.yml'):
            raise ValueError(f'Hidden source is not allowed: {name}')
        paths[name]=path
    for name in ROOT_FILES:add(ROOT/name)
    add(ROOT/'.github/workflows/release.yml')
    for name,extensions in GROUPS.items():
        base=ROOT/name
        if not base.exists():continue
        for path in base.rglob('*'):
            if not path.is_file() or path.suffix not in extensions:continue
            if '__pycache__' in path.parts:continue
            # Runtime source masters, not generated contact sheets or previews.
            if name=='assets' and path.suffix=='.png' and 'source' not in path.parts and path.parent.name!='app':continue
            add(path)
    for path in ROOT.glob('bake_*.py'):add(path)
    for path in list(paths.values()):
        if path.suffix!='.rs':continue
        for dependency in INCLUDE.findall(path.read_text(encoding='utf-8'))+LOCAL_PATH.findall(path.read_text(encoding='utf-8')):
            resolved=(path.parent/dependency).resolve()
            relative=resolved.relative_to(ROOT).as_posix()
            if relative not in paths:raise ValueError(f'{path.name} requires missing {relative}')
    total=sum(path.stat().st_size for path in paths.values())
    if total>200*1024*1024:raise ValueError('Source package exceeds 200 MiB budget')
    return dict(sorted(paths.items()))


def write_archive(paths,destination):
    destination.parent.mkdir(parents=True,exist_ok=True)
    with NamedTemporaryFile(dir=destination.parent,suffix='.zip',delete=False) as file:temporary=Path(file.name)
    try:
        with ZipFile(temporary,'w') as archive:
            for name,path in paths.items():
                entry=ZipInfo('cat-home/'+name);entry.compress_type=ZIP_DEFLATED
                archive.writestr(entry,path.read_bytes())
        with ZipFile(temporary) as archive:
            if archive.namelist()!=['cat-home/'+name for name in paths] or archive.testzip() is not None:
                raise ValueError('Archive integrity check failed')
        temporary.replace(destination)
    finally:
        # Only this exact temporary file, never a directory or collection.
        temporary.unlink(missing_ok=True)


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check',action='store_true')
    parser.add_argument('--output',type=Path,default=DEFAULT_ARCHIVE)
    parser.add_argument('--manifest',type=Path)
    args=parser.parse_args();paths=source_files()
    if args.manifest:args.manifest.write_text('\n'.join(paths)+'\n',encoding='utf-8')
    if not args.check:write_archive(paths,args.output);print('Archive:',args.output)
    print(f'Verified {len(paths)} necessary source files, {sum(p.stat().st_size for p in paths.values())/1024/1024:.2f} MiB unpacked')

if __name__=='__main__':main()
