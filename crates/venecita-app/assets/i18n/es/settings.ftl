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
drag-hint = Arrastrá sobre celdas vacías para elegir una zona. Arrastrá una regla para moverla (también a otra pantalla), o sus bordes para cambiarle el tamaño.
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

# --- Acciones (atajos) ---
dir-left = a la izquierda
dir-right = a la derecha
dir-up = arriba
dir-down = abajo
edge-left = borde izquierdo
edge-right = borde derecho
edge-up = borde superior
edge-down = borde inferior
target-next = siguiente
target-prev = anterior

action-move_zone = Mover { $dir }
action-move_zone-desc = Mueve la ventana enfocada una celda { $dir }, sin cambiar su tamaño.
action-grow = Agrandar ({ $edge })
action-grow-desc = Empuja el { $edge } de la ventana una celda hacia afuera.
action-shrink = Achicar ({ $edge })
action-shrink-desc = Mete el { $edge } de la ventana una celda hacia adentro.
action-move_to_display = Mover a la pantalla { $target }
action-move_to_display-desc = Envía la ventana enfocada a la pantalla { $target }, manteniendo su zona.
action-snap_to_grid = Ajustar a la grilla
action-snap_to_grid-desc = Lleva la ventana a la zona más cercana, sin recordarla.
action-pin_here = Fijar aquí
action-pin_here-desc = Ajusta la ventana y recuerda esa zona para su aplicación.
action-stack_next = Siguiente ventana de la zona
action-stack_next-desc = Muestra la siguiente ventana que comparte la zona de la ventana enfocada.
action-stack_prev = Ventana anterior de la zona
action-stack_prev-desc = Muestra la ventana anterior que comparte la zona de la ventana enfocada.
action-dynamic_next = Siguiente en la zona dinámica
action-dynamic_next-desc = Muestra la siguiente ventana de la zona dinámica.
action-dynamic_prev = Anterior en la zona dinámica
action-dynamic_prev-desc = Muestra la ventana anterior de la zona dinámica.
action-focus_window = Enfocar una ventana
action-focus_window-desc = Trae al frente una ventana en particular.
action-focus_mode_toggle = Modo foco
action-focus_mode_toggle-desc = Agranda la ventana enfocada y oscurece el resto. Otra vez para salir.
action-relayout_all = Ordenar ventanas
action-relayout_all-desc = Vuelve a poner cada ventana en su lugar.
action-activate_profile = Perfil «{ $name }»
action-activate_profile-desc = Cambia a este perfil (se desactiva la elección automática).
action-next_profile = Siguiente perfil
action-next_profile-desc = Pasa al siguiente perfil que sirve para las pantallas conectadas.
action-auto_profile = Perfil automático
action-auto_profile-desc = Vuelve a elegir el perfil según las pantallas conectadas.
action-toggle_pause = Pausar / reanudar
action-toggle_pause-desc = Detiene o reanuda el acomodado de ventanas.
action-open_settings = Abrir ajustes
action-open_settings-desc = Abre esta ventana.

# --- Rediseño ---
general-grid-desc = Filas × columnas en que se divide cada pantalla, salvo que tenga su propia grilla.
general-behavior = Comportamiento
general-auto-arrange = Ordenar automáticamente
general-auto-arrange-desc = Vuelve a poner una ventana en su zona apenas la movés y la soltás.
files-title = Archivos
open-config = Abrir archivo de configuración
open-logs = Ver registros
zone-title = Zona seleccionada
zone-actions = Acciones
add-app = Añadir app
rules-title = Reglas de esta distribución
connected-hint = Tocá «Asignar como…» para usar una pantalla en tus distribuciones.
profile-actions = Este perfil
creating-rule = Regla nueva: elegí sus apps y tocá Aplicar.
editing-rule = Editando la regla de { $apps }. Dibujá sobre celdas libres para una nueva.
layout-in-use = En uso ahora
layout-not-in-use = Las reglas solo se aplican en la distribución en uso, y con las pantallas conectadas ahora no es esta. En uso: { $layout }.
go-to-in-use = Ir a la que está en uso
no-layout-fits = Ninguna distribución sirve para las pantallas conectadas: revisá las pestañas Pantallas y Perfiles.
rule-app-open = Esta app está abierta ahora
