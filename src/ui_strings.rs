//! Textos de la interfaz en los cuatro idiomas, buscados por el texto en inglés.

use crate::i18n::Lang;

/// (inglés, español, portugués, chino). Los `{nombre}` se completan con `i18n::trf`.
const STRINGS: &[(&str, &str, &str, &str)] = &[
    (
        "Invalid Odoo user, key or database.",
        "Usuario, clave o base de datos de Odoo inválidos.",
        "Usuário, chave ou banco de dados do Odoo inválidos.",
        "Odoo 用户、密钥或数据库无效。",
    ),
    (
        "Access denied: {detail}",
        "Acceso denegado: {detail}",
        "Acesso negado: {detail}",
        "访问被拒绝：{detail}",
    ),
    (
        "No response in {secs}s. Check the URL and your connection.",
        "No hubo respuesta en {secs}s. Revisá la URL y tu conexión.",
        "Sem resposta em {secs}s. Confira a URL e a sua conexão.",
        "{secs} 秒内没有响应。请检查地址和网络连接。",
    ),
    (
        "Could not connect: {msg}",
        "No se pudo conectar: {msg}",
        "Não foi possível conectar: {msg}",
        "无法连接：{msg}",
    ),
    (
        "The URL answered, but it does not look like Odoo. Check the domain.",
        "La URL respondió, pero no parece ser Odoo. Revisá el dominio.",
        "A URL respondeu, mas não parece ser o Odoo. Confira o domínio.",
        "该地址有响应，但看起来不是 Odoo。请检查域名。",
    ),
    (
        "Odoo answered {code}: {detail}",
        "Odoo respondió {code}: {detail}",
        "O Odoo respondeu {code}: {detail}",
        "Odoo 返回 {code}：{detail}",
    ),
    (
        "Unexpected response from Odoo: {msg}",
        "Respuesta inesperada de Odoo: {msg}",
        "Resposta inesperada do Odoo: {msg}",
        "Odoo 返回了意外的响应：{msg}",
    ),
    ("Tasks", "Tareas", "Tarefas", "任务"),
    ("Hours", "Horas", "Horas", "工时"),
    (
        "{error} It may have been saved: check the {panel} panel before retrying.",
        "{error} Puede que se haya guardado: revisá el panel {panel} antes de reintentar.",
        "{error} Pode ter sido salvo: confira o painel {panel} antes de tentar de novo.",
        "{error} 可能已经保存：重试前请先查看“{panel}”面板。",
    ),
    (
        "unexpected response",
        "respuesta inesperada",
        "resposta inesperada",
        "意外的响应",
    ),
    ("All", "Todos", "Todos", "全部"),
    (
        "Task updated",
        "Tarea actualizada",
        "Tarefa atualizada",
        "任务已更新",
    ),
    (
        "Task created",
        "Tarea creada",
        "Tarefa criada",
        "任务已创建",
    ),
    (
        "Unexpected response from Odoo.",
        "Respuesta inesperada de Odoo.",
        "Resposta inesperada do Odoo.",
        "Odoo 返回了意外的响应。",
    ),
    (
        "Logged {hours}h on {task}",
        "Cargadas {hours}h en {task}",
        "Lançadas {hours}h em {task}",
        "已在 {task} 记录 {hours} 小时",
    ),
    (
        "Hours: no employee has your user's name; press 'm' to see all.",
        "Horas: ningún empleado se llama como tu usuario; probá 'm' para ver todas.",
        "Horas: nenhum funcionário tem o nome do seu usuário; tente 'm' para ver todas.",
        "工时：没有与你的用户同名的员工；按 'm' 查看全部。",
    ),
    (
        "Could not read your user: showing every record.",
        "No pude leer tu usuario: se muestran todos los registros.",
        "Não consegui ler o seu usuário: todos os registros são exibidos.",
        "无法读取你的用户：显示所有记录。",
    ),
    (
        "Could not locate your user: {message}",
        "No pude ubicar tu usuario: {message}",
        "Não consegui localizar o seu usuário: {message}",
        "无法定位你的用户：{message}",
    ),
    (
        "The task has no project: hours cannot be logged.",
        "La tarea no tiene proyecto: no se pueden cargar horas.",
        "A tarefa não tem projeto: não é possível lançar horas.",
        "该任务没有项目：无法记录工时。",
    ),
    (
        "Your user was not found: 'mine' is not available.",
        "No ubiqué tu usuario: 'mías' no está disponible.",
        "Não localizei o seu usuário: 'minhas' não está disponível.",
        "未找到你的用户：“我的”不可用。",
    ),
    (
        "Task completed",
        "Tarea completada",
        "Tarefa concluída",
        "任务已完成",
    ),
    (
        "The hours were already logged, but the task could not be finished: {message}",
        "Las horas ya quedaron cargadas, pero no se pudo finalizar la tarea: {message}",
        "As horas já foram lançadas, mas não foi possível finalizar a tarefa: {message}",
        "工时已记录，但无法完成该任务：{message}",
    ),
    (
        "Odoo tasks and hours, from the terminal",
        "Tareas y horas de Odoo, desde la terminal",
        "Tarefas e horas do Odoo, direto do terminal",
        "在终端里管理 Odoo 的任务和工时",
    ),
    (
        "Press any key to continue",
        "Apretá cualquier tecla para continuar",
        "Aperte qualquer tecla para continuar",
        "按任意键继续",
    ),
    (
        "Enter the Odoo database (up to 63 characters).",
        "Completá la base de datos de Odoo (hasta 63 caracteres).",
        "Preencha o banco de dados do Odoo (até 63 caracteres).",
        "请填写 Odoo 数据库（最多 63 个字符）。",
    ),
    (
        "The database accepts letters, numbers, _ - and .",
        "La base de datos admite letras, números, _ - y .",
        "O banco de dados aceita letras, números, _ - e .",
        "数据库名只能包含字母、数字以及 _ - 和 .",
    ),
    (
        "Enter the Odoo user and key.",
        "Completá el usuario y la clave de Odoo.",
        "Preencha o usuário e a chave do Odoo.",
        "请填写 Odoo 用户和密钥。",
    ),
    (
        "The URL must start with https://",
        "La URL tiene que empezar con https://",
        "A URL precisa começar com https://",
        "地址必须以 https:// 开头",
    ),
    (
        "Invalid URL: no credentials, query or spaces.",
        "URL inválida: sin credenciales, query ni espacios.",
        "URL inválida: sem credenciais, query nem espaços.",
        "地址无效：不能包含凭据、查询参数或空格。",
    ),
    (
        "The URL is just the Odoo origin (no path), e.g. https://odoo.example.com",
        "La URL es solo el origen de Odoo (sin ruta), ej: https://odoo.example.com",
        "A URL é só a origem do Odoo (sem caminho), ex.: https://odoo.example.com",
        "地址只填 Odoo 的源（不带路径），例如 https://odoo.example.com",
    ),
    (
        "The URL has no host.",
        "La URL no tiene host.",
        "A URL não tem host.",
        "地址缺少主机名。",
    ),
    (
        "https is required (http only for localhost).",
        "Se exige https (http solo para localhost).",
        "É obrigatório https (http só para localhost).",
        "必须使用 https（http 仅限 localhost）。",
    ),
    (
        "{path} has permissions {mode}: other users could read the secrets. Run: chmod 600 {path}",
        "{path} tiene permisos {mode}: otros usuarios podrían leer los secretos. Ejecutá: chmod 600 {path}",
        "{path} tem permissões {mode}: outros usuários poderiam ler os segredos. Execute: chmod 600 {path}",
        "{path} 的权限为 {mode}：其他用户可能读到密钥。请执行：chmod 600 {path}",
    ),
    (
        "Invalid config in {path}: {e}. Delete the file and enter the details again.",
        "Config inválida en {path}: {e}. Borrá el archivo y volvé a cargar los datos.",
        "Configuração inválida em {path}: {e}. Apague o arquivo e informe os dados de novo.",
        "{path} 中的配置无效：{e}。请删除该文件并重新填写。",
    ),
    (
        "Invalid config path.",
        "Ruta de config inválida.",
        "Caminho de configuração inválido.",
        "配置路径无效。",
    ),
    (
        "Could not read {path}: {e}",
        "No pude leer {path}: {e}",
        "Não consegui ler {path}: {e}",
        "无法读取 {path}：{e}",
    ),
    (
        "Could not save {path}: {e}",
        "No pude guardar {path}: {e}",
        "Não consegui salvar {path}: {e}",
        "无法保存 {path}：{e}",
    ),
    (
        "Could not create {path}: {e}",
        "No pude crear {path}: {e}",
        "Não consegui criar {path}: {e}",
        "无法创建 {path}：{e}",
    ),
    (
        "Edit task #{id}",
        "Editar tarea #{id}",
        "Editar tarefa #{id}",
        "编辑任务 #{id}",
    ),
    ("New task", "Nueva tarea", "Nova tarefa", "新建任务"),
    ("Sending…", "Enviando…", "Enviando…", "发送中…"),
    (
        "Tab/↑↓ field · Enter pick/next · Ctrl+S send · Esc cancel",
        "Tab/↑↓ campo · Enter elegir/seguir · Ctrl+S enviar · Esc cancelar",
        "Tab/↑↓ campo · Enter escolher/seguir · Ctrl+S enviar · Esc cancelar",
        "Tab/↑↓ 字段 · Enter 选择/下一个 · Ctrl+S 发送 · Esc 取消",
    ),
    (
        "DD-MM-YY (optional)",
        "DD-MM-AA (opcional)",
        "DD-MM-AA (opcional)",
        "DD-MM-YY（可选）",
    ),
    (
        "e.g. 4 (optional)",
        "ej: 4 (opcional)",
        "ex.: 4 (opcional)",
        "例如 4（可选）",
    ),
    (
        "Enter to pick",
        "Enter para elegir",
        "Enter para escolher",
        "按 Enter 选择",
    ),
    (
        "Enter toggles · Esc done",
        "Enter marca/desmarca · Esc listo",
        "Enter marca/desmarca · Esc pronto",
        "Enter 勾选/取消 · Esc 完成",
    ),
    (
        "Enter picks · Esc closes",
        "Enter elige · Esc cierra",
        "Enter escolhe · Esc fecha",
        "Enter 选择 · Esc 关闭",
    ),
    ("loading…", "cargando…", "carregando…", "加载中…"),
    (
        "(no results)",
        "(sin resultados)",
        "(sem resultados)",
        "（无结果）",
    ),
    (
        "Fill in «{label}».",
        "Completá «{label}».",
        "Preencha «{label}».",
        "请填写“{label}”。",
    ),
    (
        "«{label}» accepts up to {max} characters.",
        "«{label}» admite hasta {max} caracteres.",
        "«{label}» aceita até {max} caracteres.",
        "“{label}”最多 {max} 个字符。",
    ),
    (
        "«{label}» must be a number.",
        "«{label}» tiene que ser un número.",
        "«{label}» precisa ser um número.",
        "“{label}”必须是数字。",
    ),
    (
        "«{label}» cannot be negative.",
        "«{label}» no puede ser negativo.",
        "«{label}» não pode ser negativo.",
        "“{label}”不能为负数。",
    ),
    (
        "Invalid deadline: use DD-MM-YY (e.g. 20-11-26).",
        "Deadline inválido: usá DD-MM-AA (ej: 20-11-26).",
        "Prazo inválido: use DD-MM-AA (ex.: 20-11-26).",
        "截止日期无效：请用 DD-MM-YY（例如 20-11-26）。",
    ),
    (
        "Invalid deadline.",
        "Deadline inválido.",
        "Prazo inválido.",
        "截止日期无效。",
    ),
    (
        "Ambiguous deadline because of a daylight-saving change.",
        "Deadline ambiguo por cambio de horario.",
        "Prazo ambíguo por mudança de horário de verão.",
        "因夏令时切换，截止日期有歧义。",
    ),
    (
        "No changes to save.",
        "No hay cambios para guardar.",
        "Não há alterações para salvar.",
        "没有需要保存的更改。",
    ),
    (
        "Invalid date: use DD-MM-YY (e.g. 05-11-26).",
        "Fecha inválida: usá DD-MM-AA (ej: 05-11-26).",
        "Data inválida: use DD-MM-AA (ex.: 05-11-26).",
        "日期无效：请用 DD-MM-YY（例如 05-11-26）。",
    ),
    (
        "The date cannot be in the future.",
        "La fecha no puede ser futura.",
        "A data não pode ser futura.",
        "日期不能是未来。",
    ),
    (
        "Invalid start time: use HH:MM (e.g. 10:30).",
        "Hora de inicio inválida: usá HH:MM (ej: 10:30).",
        "Hora de início inválida: use HH:MM (ex.: 10:30).",
        "开始时间无效：请用 HH:MM（例如 10:30）。",
    ),
    (
        "The start time cannot be in the future.",
        "La hora de inicio no puede ser futura.",
        "A hora de início não pode ser futura.",
        "开始时间不能是未来。",
    ),
    (
        "Invalid hours: e.g. 1.5",
        "Horas inválidas: ej. 1.5",
        "Horas inválidas: ex.: 1.5",
        "工时无效：例如 1.5",
    ),
    (
        "Hours must be greater than 0 and at most 24.",
        "Las horas tienen que ser mayores a 0 y hasta 24.",
        "As horas precisam ser maiores que 0 e no máximo 24.",
        "工时必须大于 0 且不超过 24。",
    ),
    (
        "Write a description.",
        "Escribí una descripción.",
        "Escreva uma descrição.",
        "请填写描述。",
    ),
    (
        "The description accepts up to {max} characters.",
        "La descripción admite hasta {max} caracteres.",
        "A descrição aceita até {max} caracteres.",
        "描述最多 {max} 个字符。",
    ),
    (
        "The start time is ambiguous because of a daylight-saving change.",
        "La hora de inicio es ambigua por cambio de horario.",
        "A hora de início é ambígua por mudança de horário de verão.",
        "因夏令时切换，开始时间有歧义。",
    ),
    (
        "Log hours ·",
        "Cargar horas ·",
        "Lançar horas ·",
        "记录工时 ·",
    ),
    ("Description", "Descripción", "Descrição", "描述"),
    (
        "Finish task?",
        "Finalizar tarea?",
        "Finalizar tarefa?",
        "完成任务？",
    ),
    (
        "Space toggles: moves it to Done",
        "Espacio marca: pasa a Hecha",
        "Espaço marca: vai para Concluída",
        "空格切换：任务变为“已完成”",
    ),
    (
        "This Odoo does not store a start time (it has no date_time field).",
        "Este Odoo no guarda hora de inicio (no tiene el campo date_time).",
        "Este Odoo não guarda hora de início (não tem o campo date_time).",
        "此 Odoo 不保存开始时间（没有 date_time 字段）。",
    ),
    (
        "Tab field · Enter next/send · Ctrl+S send · Esc cancel",
        "Tab campo · Enter seguir/enviar · Ctrl+S enviar · Esc cancelar",
        "Tab campo · Enter seguir/enviar · Ctrl+S enviar · Esc cancelar",
        "Tab 字段 · Enter 下一个/发送 · Ctrl+S 发送 · Esc 取消",
    ),
    (
        "(no description)",
        "(sin descripción)",
        "(sem descrição)",
        "（无描述）",
    ),
    ("(image only)", "(solo imagen)", "(só imagem)", "（仅图片）"),
    (
        "(the description has images)",
        "(la descripción tiene imágenes)",
        "(a descrição tem imagens)",
        "（描述中含有图片）",
    ),
    (
        "{allocated} allocated · {logged} logged · {remaining} remaining",
        "{allocated} asignadas · {logged} cargadas · {remaining} restantes",
        "{allocated} alocadas · {logged} lançadas · {remaining} restantes",
        "预计 {allocated} · 已记录 {logged} · 剩余 {remaining}",
    ),
    (
        "unexpected HTTP response from Odoo",
        "respuesta HTTP inesperada de Odoo",
        "resposta HTTP inesperada do Odoo",
        "Odoo 返回了意外的 HTTP 响应",
    ),
    (
        "invalid JSON-RPC response",
        "respuesta JSON-RPC inválida",
        "resposta JSON-RPC inválida",
        "JSON-RPC 响应无效",
    ),
    (
        "incomplete JSON-RPC response",
        "respuesta JSON-RPC incompleta",
        "resposta JSON-RPC incompleta",
        "JSON-RPC 响应不完整",
    ),
    (
        "Only internal users can use lazyodoo.",
        "Solo los usuarios internos pueden usar lazyodoo.",
        "Só usuários internos podem usar o lazyodoo.",
        "只有内部用户才能使用 lazyodoo。",
    ),
    (
        "Could not read your user.",
        "No se pudo leer tu usuario.",
        "Não foi possível ler o seu usuário.",
        "无法读取你的用户。",
    ),
    (
        "You do not have permission for that operation.",
        "No tenés permiso para esa operación.",
        "Você não tem permissão para essa operação.",
        "你没有执行该操作的权限。",
    ),
    (
        "Odoo could not complete the operation. {message}",
        "Odoo no pudo completar la operación. {message}",
        "O Odoo não conseguiu concluir a operação. {message}",
        "Odoo 无法完成该操作。{message}",
    ),
    (
        "This Odoo has no «date_time» field on timesheets: it does not store a start time",
        "Este Odoo no tiene el campo «date_time» en las horas: no guarda hora de inicio",
        "Este Odoo não tem o campo «date_time» nas horas: não guarda hora de início",
        "此 Odoo 的工时没有“date_time”字段：不保存开始时间",
    ),
    (
        "Odoo did not return the id of the created record",
        "Odoo no devolvió el id del registro creado",
        "O Odoo não devolveu o id do registro criado",
        "Odoo 没有返回新建记录的 id",
    ),
    (
        "This Odoo has no «state» field on tasks: change the stage with «s»",
        "Este Odoo no tiene el campo «state» en las tareas: cambiá el estado con «s»",
        "Este Odoo não tem o campo «state» nas tarefas: mude a etapa com «s»",
        "此 Odoo 的任务没有“state”字段：请按“s”更改阶段",
    ),
    (
        "The change was saved, but the record could not be read back.",
        "El cambio se guardó, pero no se pudo leer el registro de vuelta.",
        "A alteração foi salva, mas não foi possível ler o registro de volta.",
        "更改已保存，但无法重新读取该记录。",
    ),
    ("Projects", "Proyectos", "Projetos", "项目"),
    (
        "Hours this week",
        "Horas semana",
        "Horas da semana",
        "本周工时",
    ),
    ("Detail", "Detalle", "Detalhe", "详情"),
    ("(empty)", "(vacío)", "(vazio)", "（空）"),
    ("mine", "mías", "minhas", "我的"),
    ("all", "todas", "todas", "全部"),
    ("all projects", "todos", "todos", "全部"),
    ("project", "proyecto", "projeto", "项目"),
    (
        "type to filter · Enter apply · Esc clear",
        "escribí para filtrar · Enter aplicar · Esc limpiar",
        "digite para filtrar · Enter aplicar · Esc limpar",
        "输入以筛选 · Enter 应用 · Esc 清除",
    ),
    (
        "1-3 panel · j/k move · Enter apply project · m mine/all · / filter · r refresh · l language · q quit",
        "1-3 panel · j/k mover · Enter aplicar proyecto · m mías/todas · / filtrar · r refrescar · l idioma · q salir",
        "1-3 painel · j/k mover · Enter aplicar projeto · m minhas/todas · / filtrar · r atualizar · l idioma · q sair",
        "1-3 面板 · j/k 移动 · Enter 应用项目 · m 我的/全部 · / 筛选 · r 刷新 · l 语言 · q 退出",
    ),
    (
        "j/k move · t hours · n new · e edit · c complete · a assign · s stage · p prio · l language · q quit",
        "j/k mover · t horas · n nueva · e editar · c completar · a asignar · s estado · p prio · l idioma · q salir",
        "j/k mover · t horas · n nova · e editar · c concluir · a atribuir · s etapa · p prio · l idioma · q sair",
        "j/k 移动 · t 工时 · n 新建 · e 编辑 · c 完成 · a 指派 · s 阶段 · p 优先级 · l 语言 · q 退出",
    ),
    (
        "1-3 panel · j/k move · J/K detail · m mine/all · / filter · r refresh · l language · q quit",
        "1-3 panel · j/k mover · J/K detalle · m mías/todas · / filtrar · r refrescar · l idioma · q salir",
        "1-3 painel · j/k mover · J/K detalhe · m minhas/todas · / filtrar · r atualizar · l idioma · q sair",
        "1-3 面板 · j/k 移动 · J/K 详情 · m 我的/全部 · / 筛选 · r 刷新 · l 语言 · q 退出",
    ),
    (
        "Complete task",
        "Completar tarea",
        "Concluir tarefa",
        "完成任务",
    ),
    ("Saving…", "Guardando…", "Salvando…", "保存中…"),
    (
        "Hours logged. Finishing the task…",
        "Horas cargadas. Finalizando la tarea…",
        "Horas lançadas. Finalizando a tarefa…",
        "工时已记录。正在完成任务…",
    ),
    (
        "Mark as completed?",
        "¿Marcar como completada?",
        "Marcar como concluída?",
        "标记为已完成？",
    ),
    (
        "Hours logged: {hours} h",
        "Horas cargadas: {hours} h",
        "Horas lançadas: {hours} h",
        "已记录工时：{hours} 小时",
    ),
    (
        "Enter retry · Esc close",
        "Enter reintentar · Esc cerrar",
        "Enter tentar de novo · Esc fechar",
        "Enter 重试 · Esc 关闭",
    ),
    (
        "Enter confirm · Esc cancel",
        "Enter confirmar · Esc cancelar",
        "Enter confirmar · Esc cancelar",
        "Enter 确认 · Esc 取消",
    ),
    (
        "first start",
        "primer arranque",
        "primeira execução",
        "首次启动",
    ),
    (
        "Enter your access details. They are checked against Odoo before being saved.",
        "Cargá los datos de acceso. Se validan contra Odoo antes de guardarlos.",
        "Informe os dados de acesso. Eles são validados no Odoo antes de serem salvos.",
        "请填写登录信息。保存前会先向 Odoo 验证。",
    ),
    (
        "Checking against Odoo…",
        "Validando contra Odoo…",
        "Validando no Odoo…",
        "正在向 Odoo 验证…",
    ),
    (
        "Tab/↑↓ field · Enter next/confirm · Esc quit",
        "Tab/↑↓ campo · Enter seguir/confirmar · Esc salir",
        "Tab/↑↓ campo · Enter seguir/confirmar · Esc sair",
        "Tab/↑↓ 字段 · Enter 下一个/确认 · Esc 退出",
    ),
    (
        "The check ended unexpectedly.",
        "La validación terminó mal.",
        "A validação terminou de forma inesperada.",
        "验证意外结束。",
    ),
    ("Odoo URL", "URL de Odoo", "URL do Odoo", "Odoo 地址"),
    ("Database", "Base de datos", "Banco de dados", "数据库"),
    (
        "Odoo user",
        "Usuario de Odoo",
        "Usuário do Odoo",
        "Odoo 用户",
    ),
    (
        "Odoo API key",
        "API key de Odoo",
        "API key do Odoo",
        "Odoo API 密钥",
    ),
    (
        "name@example.com",
        "nombre@example.com",
        "nome@example.com",
        "name@example.com",
    ),
    ("Name", "Nombre", "Nome", "名称"),
    ("Project", "Proyecto", "Projeto", "项目"),
    ("Assignees", "Asignados", "Responsáveis", "负责人"),
    ("Stage", "Estado", "Etapa", "阶段"),
    ("Deadline", "Deadline", "Prazo", "截止日期"),
    (
        "Allocated hours",
        "Horas asignadas",
        "Horas alocadas",
        "预计工时",
    ),
    ("Priority", "Prioridad", "Prioridade", "优先级"),
    ("Tags", "Tags", "Tags", "标签"),
    ("Normal", "Normal", "Normal", "普通"),
    ("High", "Alta", "Alta", "高"),
    ("Date", "Fecha", "Data", "日期"),
    ("Start time", "Hora de inicio", "Hora de início", "开始时间"),
    ("Start", "Inicio", "Início", "开始"),
    ("Customer", "Cliente", "Cliente", "客户"),
    ("Employee", "Empleado", "Funcionário", "员工"),
    ("Task", "Tarea", "Tarefa", "任务"),
    (
        "DD-MM-YY, up to today",
        "DD-MM-AA, hasta hoy",
        "DD-MM-AA, até hoje",
        "DD-MM-YY，最晚今天",
    ),
    ("HH:MM", "HH:MM", "HH:MM", "HH:MM"),
    (
        "0-24, e.g. 1.5",
        "0 a 24, ej: 1.5",
        "0 a 24, ex. 1.5",
        "0-24，如 1.5",
    ),
];

pub fn lookup(lang: Lang, en: &str) -> Option<&'static str> {
    let row = STRINGS.iter().find(|row| row.0 == en)?;
    Some(match lang {
        Lang::En => row.0,
        Lang::Es => row.1,
        Lang::Pt => row.2,
        Lang::Zh => row.3,
    })
}

#[cfg(test)]
pub fn all() -> &'static [(&'static str, &'static str, &'static str, &'static str)] {
    STRINGS
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn placeholders(text: &str) -> BTreeSet<String> {
        text.split('{')
            .skip(1)
            .filter_map(|rest| rest.split_once('}').map(|(name, _)| name.to_string()))
            .collect()
    }

    #[test]
    fn every_text_is_translated_with_the_same_placeholders() {
        let mut seen = BTreeSet::new();
        for (en, es, pt, zh) in all() {
            assert!(seen.insert(*en), "clave repetida: {en}");
            for (lang, text) in [("es", es), ("pt", pt), ("zh", zh)] {
                assert!(!text.trim().is_empty(), "falta {lang}: {en}");
                assert_eq!(placeholders(en), placeholders(text), "{lang}: {en}");
            }
        }
    }

    /// Literales que se pasan a `tr(...)`/`trf(...)` en el código (sin los tests).
    fn keys_used_in_code() -> BTreeSet<String> {
        let sources = [
            include_str!("api.rs"),
            include_str!("app.rs"),
            include_str!("banner.rs"),
            include_str!("config.rs"),
            include_str!("create_form.rs"),
            include_str!("hours_form.rs"),
            include_str!("main.rs"),
            include_str!("models.rs"),
            include_str!("odoo.rs"),
            include_str!("routes.rs"),
            include_str!("ui.rs"),
            include_str!("wizard.rs"),
        ];
        let mut keys = BTreeSet::new();
        for source in sources {
            let code = source.split("#[cfg(test)]\nmod tests").next().unwrap();
            for call in ["tr(", "trf(", "io_error("] {
                for (at, _) in code.match_indices(call) {
                    let before = code[..at].chars().last();
                    if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                        continue;
                    }
                    let rest = code[at + call.len()..].trim_start();
                    if let Some(rest) = rest.strip_prefix('"') {
                        let mut end = 0;
                        let bytes = rest.as_bytes();
                        while end < bytes.len() && bytes[end] != b'"' {
                            end += if bytes[end] == b'\\' { 2 } else { 1 };
                        }
                        keys.insert(rest[..end].replace("\\\"", "\""));
                    }
                }
            }
        }
        keys
    }

    #[test]
    fn every_text_used_in_the_code_has_a_translation() {
        // los que se piden por una variable en vez de un literal
        let dynamic = [
            "Odoo URL",
            "Database",
            "Odoo user",
            "Odoo API key",
            "name@example.com",
            "Name",
            "Project",
            "Assignees",
            "Stage",
            "Deadline",
            "Allocated hours",
            "Priority",
            "Tags",
            "Normal",
            "High",
            "Date",
            "Start time",
            "Start",
            "Customer",
            "Employee",
            "Task",
            "DD-MM-YY, up to today",
            "HH:MM",
            "0-24, e.g. 1.5",
            "Hours",
            "No changes to save.",
        ];
        let known: BTreeSet<&str> = all().iter().map(|row| row.0).collect();
        let mut used = keys_used_in_code();
        used.extend(dynamic.iter().map(|k| k.to_string()));
        used.retain(|key| !key.is_empty());
        for key in &used {
            assert!(known.contains(key.as_str()), "sin traducción: {key}");
        }
        for key in &known {
            assert!(used.contains(*key), "traducción sin uso: {key}");
        }
    }
}
