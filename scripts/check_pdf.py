"""Independent Poppler/pypdf qualification of generated ProcessCraft PDFs."""
import json, math, subprocess
from pathlib import Path
from PIL import Image, ImageChops, ImageStat
from pypdf import PdfReader

root = Path('target/m3-qualification/pdf')
manifest = json.loads((root / 'manifest.json').read_text(encoding='utf-8-sig'))
report = {'checks': [], 'limitations': ['Raster comparison is limited to the supplied deterministic fixtures; anti-aliasing differs between Poppler and tiny-skia.', 'Text is outlines and therefore is not searchable, selectable or tagged.']}

def check(name, condition, detail):
    report['checks'].append({'check': name, 'pass': bool(condition), 'detail': detail})
    print('PDF_CHECK', 'PASS' if condition else 'FAIL', name, detail)

check('page cap refused', manifest['page_cap_refused'], 'Export rejected the >100-page fixture')
for sample in manifest['samples']:
    name = sample['name']; path = root / f'{name}.pdf'
    reader = PdfReader(path, strict=True)
    pdfinfo = subprocess.run(['pdfinfo', str(path)], check=True, text=True, capture_output=True).stdout
    scale = 0.75; tile_w = (841.89 - 48) / scale; tile_h = (595.28 - 48) / scale
    width, height = sample['logical_size']; columns = math.ceil(width / tile_w); rows = math.ceil(height / tile_h)
    check(f'{name}: pages', len(reader.pages) == columns * rows, f'{len(reader.pages)} pages, expected {columns * rows}')
    check(f'{name}: A4 landscape', all(abs(float(p.mediabox.width)-841.89)<0.02 and abs(float(p.mediabox.height)-595.28)<0.02 for p in reader.pages), '841.89 x 595.28 pt')
    check(f'{name}: ack', sample['ack_refused'] and any('outlined' in d for d in sample['diagnostics']), 'unacknowledged PDF refused; outline notice present')
    all_forms=[]; images=[]; fonts=[]
    for page in reader.pages:
        resources=page['/Resources']; fonts.extend(resources.get('/Font', {}).keys())
        for ref in resources.get('/XObject', {}).values():
            obj=ref.get_object(); subtype=obj.get('/Subtype'); all_forms.append(str(subtype));
            if subtype == '/Image': images.append(str(subtype))
            nested=obj.get('/Resources',{}).get('/XObject',{})
            for n in nested.values():
                if n.get_object().get('/Subtype') == '/Image': images.append('/Image')
    check(f'{name}: vector/no embedded fonts', not images and not fonts and all_forms and all(x=='/Form' for x in all_forms), f'forms={len(all_forms)}, images={len(images)}, fonts={len(fonts)}')
    check(f'{name}: no searchable text', all(not (p.extract_text() or '').strip() for p in reader.pages), 'pypdf extracts no text')
    check(f'{name}: Poppler parse', f'Pages:           {len(reader.pages)}' in pdfinfo and 'JavaScript:      no' in pdfinfo, 'pdfinfo parsed each file; JavaScript absent')
    prefix=root/f'{name}-page';subprocess.run(['pdftoppm','-r','96','-png',str(path),str(prefix)],check=True,capture_output=True)
    rendered=sorted(root.glob(f'{name}-page-*.png'))
    check(f'{name}: raster pages', len(rendered)==len(reader.pages), f'{len(rendered)} rasterized pages')
    if len(reader.pages)>1:
        original=Image.open(root/f'{name}.png').convert('RGB')
        errors=[]
        for index, raster in enumerate(rendered):
            # At 96 dpi one scene pixel maps to one raster pixel. Ignore two
            # pixels at tile boundaries, where fractional page origins alias.
            ox=round((index % columns)*tile_w); oy=round((index // columns)*tile_h)
            tw=min(math.floor(tile_w)-4,original.width-ox-4); th=min(math.floor(tile_h)-4,original.height-oy-4)
            if tw <= 0 or th <= 0: continue
            page=Image.open(raster).convert('RGB')
            pdf_crop=page.crop((34,34,34+tw,34+th)); scene_crop=original.crop((ox+2,oy+2,ox+2+tw,oy+2+th))
            errors.append(sum(ImageStat.Stat(ImageChops.difference(pdf_crop,scene_crop)).mean)/3)
        check(f'{name}: tiled scene parity',len(errors)==len(reader.pages) and max(errors)<3.0,f'per-page mean RGB differences: {[round(e,3) for e in errors]}; excludes 2px tile seams')
    if len(reader.pages)==1:
        page=Image.open(rendered[0]).convert('RGB'); original=Image.open(root/f'{name}.png').convert('RGB')
        crop=page.crop((32,32,32+original.width,32+original.height));crop.save(root/f'{name}-pdf-crop.png')
        diff=ImageChops.difference(crop,original); mean=sum(ImageStat.Stat(diff).mean)/3
        check(f'{name}: PNG scene parity',mean<3.0,f'mean absolute RGB difference {mean:.3f}/255 (Poppler vs tiny-skia)')
        extrema=ImageStat.Stat(page.crop((0,0,page.width,30))).extrema
        check(f'{name}: top margin',all(lo>=250 for lo,hi in extrema),f'pixel extrema {extrema}')
(root/'report.json').write_text(json.dumps(report,indent=2),encoding='utf-8')
if not all(c['pass'] for c in report['checks']): raise SystemExit(1)
