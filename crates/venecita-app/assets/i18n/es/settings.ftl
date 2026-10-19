# Venecita Dock — ventana de ajustes (Español)
# Cada id de mensaje aquí debe existir también en en/settings.ftl (lo verifica un test).

window-title = Ajustes de Venecita Dock

# Acciones compartidas
save = Guardar
cancel = Cancelar
close = Cerrar
add = Añadir
remove = Quitar
delete = Eliminar
rename = Renombrar
duplicate = Duplicar
edit = Editar

unsaved-changes = Tenés cambios sin guardar.
save-failed = No se pudo guardar: { $error }

# Pestañas
tab-layout = Distribución
tab-displays = Pantallas
tab-profiles = Perfiles
tab-hotkeys = Atajos
tab-general = General

# --- Editor de distribución ---
layout-editor-title = Editor de distribución
select-profile = Perfil
select-layout = Distribución
dynamic-zone = Zona dinámica
mark-dynamic = Marcar la selección como zona dinámica
clear-dynamic = Quitar la zona dinámica
zone-apps = Aplicaciones en esta zona
running-apps = Aplicaciones abiertas
type-app = Escribí un id o nombre de app
drag-hint = Arrastrá sobre la grilla para elegir una zona.
no-selection = Seleccioná una zona en una pantalla para añadir o editar una regla.
rule-count = { $count } regla(s)
new-rule = Nueva regla
apply-rule = Aplicar

# --- Pantallas ---
displays-title = Pantallas
connected-displays = Pantallas conectadas
defined-displays = Definidas en tu configuración
identify = Identificar
identify-hint = Muestra un número por un momento en cada pantalla.
assign-as = Asignar como…
display-name = Nombre
display-resolution = Resolución
display-builtin = Integrada
display-external = Externa
display-grid = Grilla

# --- Perfiles ---
profiles-title = Perfiles
new-profile = Nuevo perfil
profile-name = Nombre del perfil
layouts = Distribuciones
new-layout = Nueva distribución
layout-name = Nombre de la distribución
layout-displays = Pantallas usadas
no-profiles = Todavía no hay perfiles. Creá uno para empezar.

# --- Atajos ---
hotkeys-title = Atajos de teclado
col-action = Acción
col-shortcut = Atajo
press-keys = Presioná las teclas…
conflict-warning = Este atajo ya lo usa otra acción.
reset-hotkeys = Restaurar los predeterminados

# --- General ---
general-title = General
general-language = Idioma
lang-auto = Automático (según el sistema)
lang-en = English
lang-es = Español
general-grid = Grilla predeterminada
grid-cols = Columnas
grid-rows = Filas
general-gap = Espacio entre ventanas
general-launch-at-login = Iniciar con el sistema
general-ignore = Aplicaciones ignoradas
ignore-hint = Estas aplicaciones nunca se mueven.

focus-title = Modo foco
focus-size = Tamaño de la ventana enfocada
focus-dim = Opacidad del oscurecido

dim-title = Oscurecer las demás ventanas
dim-enabled = Oscurecer todas las ventanas salvo la enfocada
dim-opacity = Intensidad

api-title = API local
api-enabled = Activar la API local
api-bind = Dirección de escucha
api-allow-lan = Permitir conexiones desde la red local
api-lan-warning = ⚠ Exponer la API a la red permite que otros dispositivos controlen tus ventanas. Mantené el token en secreto.
api-token = Token de acceso
token-show = Mostrar
token-hide = Ocultar
token-copy = Copiar
token-rotate = Rotar
token-rotated = Se generó un token nuevo.

# --- Marco de la ventana ---
saved = Guardado.
discard = Descartar
problems = { $count } problema(s) para corregir
load-failed = No se pudo leer el archivo de configuración
open-file = Abrir el archivo en un editor de texto
no-layouts = Este perfil todavía no tiene distribuciones. Añadí una en la pestaña Perfiles.
displays-matched-as = Definida como
display-unmatched = sin definir
capture-hint = Hacé clic en un atajo y presioná las teclas nuevas (Esc cancela). Si el sistema o el propio Venecita capturan las teclas, escribí el atajo.
token-missing = Todavía no hay token.
