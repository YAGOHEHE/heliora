"""Pruebas de validación. Cada fase tiene su archivo."""
import importlib.util

for _n in ("pruebas_fase1", "pruebas_fase1b", "pruebas_fase2", "pruebas_fase2b", "pruebas_fase3", "pruebas_fase4", "pruebas_cruces", "pruebas_linealizacion",
           "pruebas_senal", "pruebas_termica", "pruebas_almacenamiento", "pruebas_maquinas",
           "pruebas_correcciones", "pruebas_herramientas"):
    _nombre = f"{__package__}.{_n}"
    if importlib.util.find_spec(_nombre) is not None:
        importlib.import_module(_nombre)
