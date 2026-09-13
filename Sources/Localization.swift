import Foundation

public enum AppLanguage: String, CaseIterable, Sendable {
    case en = "en"
    case uk = "uk"
    case ru = "ru"
    case de = "de"
    case fr = "fr"
    case es = "es"
    case it = "it"
    case pt = "pt"
    case pl = "pl"
    case nl = "nl"
    case ja = "ja"
    case zhHans = "zh-Hans"
    case vi = "vi"

    public var displayName: String {
        switch self {
        case .en: return "English"
        case .uk: return "Українська"
        case .ru: return "Русский"
        case .de: return "Deutsch"
        case .fr: return "Français"
        case .es: return "Español"
        case .it: return "Italiano"
        case .pt: return "Português"
        case .pl: return "Polski"
        case .nl: return "Nederlands"
        case .ja: return "日本語"
        case .zhHans: return "简体中文"
        case .vi: return "Tiếng Việt"
        }
    }
}

public final class LocalizationManager: @unchecked Sendable {
    public static let shared = LocalizationManager()

    private let lock = NSLock()
    private var overrideLanguage: AppLanguage?
    private var customBundle: Bundle?

    private init() {}

    public func setOverrideLanguage(_ language: AppLanguage?) {
        lock.lock()
        defer { lock.unlock() }
        overrideLanguage = language
    }

    public func setCustomBundle(_ bundle: Bundle?) {
        lock.lock()
        defer { lock.unlock() }
        customBundle = bundle
    }

    public var currentLanguage: AppLanguage {
        lock.lock()
        defer { lock.unlock() }
        if let override = overrideLanguage {
            return override
        }
        return Self.detectSystemLanguage()
    }

    public static func detectSystemLanguage(preferences: [String] = Locale.preferredLanguages) -> AppLanguage {
        guard !preferences.isEmpty else { return .en }

        // Check user preferences directly in order of priority
        for pref in preferences {
            let lower = pref.lowercased()
            if lower.hasPrefix("ja") { return .ja }
            if lower.hasPrefix("zh") { return .zhHans }
            if lower.hasPrefix("vi") { return .vi }
            if lower.hasPrefix("uk") { return .uk }
            if lower.hasPrefix("ru") { return .ru }
            if lower.hasPrefix("de") { return .de }
            if lower.hasPrefix("fr") { return .fr }
            if lower.hasPrefix("es") { return .es }
            if lower.hasPrefix("it") { return .it }
            if lower.hasPrefix("pt") { return .pt }
            if lower.hasPrefix("pl") { return .pl }
            if lower.hasPrefix("nl") { return .nl }
            if lower.hasPrefix("en") { return .en }
        }

        let available = AppLanguage.allCases.map { $0.rawValue }
        let matched = Bundle.preferredLocalizations(from: available, forPreferences: preferences)
        if let first = matched.first {
            if let lang = AppLanguage(rawValue: first) {
                return lang
            }
            if first.hasPrefix("zh") {
                return .zhHans
            }
        }
        return .en
    }

    public func string(forKey key: String) -> String {
        let lang = currentLanguage

        let bundle = {
            lock.lock()
            defer { lock.unlock() }
            return customBundle ?? Bundle.main
        }()

        if let path = bundle.path(forResource: lang.rawValue, ofType: "lproj"),
           let langBundle = Bundle(path: path) {
            let localized = langBundle.localizedString(forKey: key, value: nil, table: nil)
            if localized != key {
                return localized
            }
        }

        if let dict = Self.translations[lang], let val = dict[key] {
            return val
        }
        if let enDict = Self.translations[.en], let val = enDict[key] {
            return val
        }
        return key
    }

    public static let translations: [AppLanguage: [String: String]] = [
        .ru: [
            "menu_title": "OpenAI Codex Quota Monitor",
            "legend_circles": "Верх = 5ч Спринт · Центр = Недельный лимит · Низ = Кредиты сброса",
            "active_in_cli": "[АКТИВЕН В CLI]",
            "active_in_app": "[АКТИВЕН В APP]",
            "reserve_slot": "[РЕЗЕРВ #%d]",
            "five_hour_sprint": "5ч-спринт:  %@  ",
            "weekly_limit": "Недельный лимит:  %@  ",
            "reset_credits": "Кредиты сброса",
            "reset_in_hours_minutes": "(сброс через %dч %dм)",
            "reset_in_minutes": "(сброс через %dм)",
            "reset_now": "сброс сейчас",
            "duration_days_hours": "%dд %dч",
            "models_menu_title": "Выбор модели CLI",
            "last_updated": "🕐 Обновлено: %@",
            "refresh_now": "🔄 Обновить сейчас",
            "refreshing": "🔄 Обновление...",
            "refresh_interval": "⏱ Интервал обновления",
            "minutes_short": "%d мин",
            "help_guide": "📖 Справка и руководство",
            "restart_app": "🚀 Перезапустить Codex App",
            "launch_at_login": "Запускать при входе",
            "quit": "Выход",
            "remove_account": "🗑 Удалить аккаунт...",
            "remove_account_title": "Удалить аккаунт",
            "remove_account_confirm": "Вы уверены, что хотите удалить %@ из Codex Monitor?",
            "remove_confirm_btn": "Удалить",
            "cancel_btn": "Отмена",
            "add_account": "➕ Добавить аккаунт...",
            "add_account_title": "Добавить аккаунт Codex",
            "add_account_msg": "Введите идентификатор аккаунта (например: personal, work, team-2):",
            "add_account_login_browser": "Войти через браузер",
            "add_account_save_current": "Сохранить текущую сессию",
            "add_account_browser_prompt": "Открываем браузер для входа. Пожалуйста, завершите авторизацию...",
            "add_account_success": "Аккаунт '%@' успешно добавлен!",
            "add_account_saved_success": "Текущая сессия сохранена как '%@'!",
            "add_account_failed_title": "Ошибка добавления аккаунта",
            "add_account_failed_desc": "Не удалось добавить аккаунт. Попробуйте еще раз.",
            "switch_to_account": "Переключиться на этот аккаунт",
            "cli_up_to_date": "✓ Codex CLI v%@ (актуальная версия)",
            "cli_update_available": "🚀 Обновить Codex CLI (до v%@)",
            "cli_not_found": "⚠️ Codex CLI не найден",
            "rename_account": "✏️ Изменить никнейм...",
            "rename_account_title": "Изменить никнейм аккаунта",
            "rename_account_prompt": "Введите отображаемый никнейм для %@:",
            "save_btn": "Сохранить",
            "clear_btn": "Сбросить",
            "restart_app_on_switch": "🚀 Перезапускать Codex App при смене",
            "stack_percentages": "Компактный стек процентов (2 строки)",
            "auto_switch_on_limit": "🔄 Автопереключение при исчерпании лимита",
            "auto_switch_business_only": "🏢 Автопереход только по бизнес-аккаунтам",
            "auto_switch_business_priority": "⚡ Автопереход с приоритетом бизнес-аккаунтов"
        ],
        .en: [
            "menu_title": "OpenAI Codex Quota Monitor",
            "legend_circles": "Top = 5h Sprint · Middle = Weekly Limit · Bottom = Reset Credits",
            "active_in_cli": "[ACTIVE IN CLI]",
            "active_in_app": "[ACTIVE IN APP]",
            "reserve_slot": "[RESERVE #%d]",
            "five_hour_sprint": "5h sprint:  %@  ",
            "weekly_limit": "Weekly limit:  %@  ",
            "reset_credits": "Reset Credits",
            "reset_in_hours_minutes": "(resets in %dh %dm)",
            "reset_in_minutes": "(resets in %dm)",
            "reset_soon": "(resets soon)",
            "reset_now": "(resets now)",
            "hours_short": "%dh",
            "hours_minutes_short": "%dh %dm",
            "minutes_short": "%d min",
            "duration_days_hours": "%dd %dh",
            "duration_hours_minutes": "%dh %dm",
            "duration_minutes": "%dm",
            "help_guide": "📖 Help & Documentation",
            "restart_app": "🚀 Restart Codex Desktop App",
            "launch_at_login": "Launch at Login",
            "quit": "Quit",
            "remove_account": "🗑 Remove Account...",
            "remove_account_title": "Remove Account",
            "remove_account_confirm": "Are you sure you want to remove %@ from Codex Monitor?",
            "remove_confirm_btn": "Remove",
            "cancel_btn": "Cancel",
            "add_account": "➕ Add Account...",
            "add_account_title": "Add Codex Account",
            "add_account_msg": "Enter an account identifier (e.g. personal, work, secondary):",
            "add_account_login_browser": "Log In in Browser",
            "add_account_save_current": "Save Current Session",
            "add_account_browser_prompt": "Opening browser for login. Please complete authentication in your browser...",
            "add_account_success": "Account '%@' added successfully!",
            "add_account_saved_success": "Current session saved as '%@'!",
            "add_account_failed_title": "Failed to Add Account",
            "add_account_failed_desc": "Could not complete account setup. Please check credentials and try again.",
            "switch_to_account": "Switch to this account",
            "cli_up_to_date": "✓ Codex CLI v%@ (up to date)",
            "cli_update_available": "🚀 Update Codex CLI (to v%@)",
            "cli_not_found": "⚠️ Codex CLI not found",
            "rename_account": "✏️ Edit Nickname...",
            "rename_account_title": "Edit Account Nickname",
            "rename_account_prompt": "Enter a display nickname for %@:",
            "save_btn": "Save",
            "clear_btn": "Clear",
            "restart_app_on_switch": "🚀 Restart Codex App on Switch",
            "stack_percentages": "Stack Percentages (2-Row)",
            "auto_switch_on_limit": "🔄 Auto-Switch on Limit Depletion",
            "auto_switch_business_only": "🏢 Auto-Switch Business Accounts Only",
            "auto_switch_business_priority": "⚡ Auto-Switch with Business Account Priority"
        ],
        .uk: [
            "menu_title": "OpenAI Codex Quota Monitor",
            "legend_circles": "5г Спринт · Тижневий ліміт · Кредити",
            "active_in_cli": "[АКТИВНИЙ В CLI]",
            "active_in_app": "[АКТИВНИЙ В APP]",
            "reserve_slot": "[РЕЗЕРВ #%d]",
            "five_hour_sprint": "5г-спринт:  %@  ",
            "weekly_limit": "Тижневий ліміт:  %@  ",
            "reset_credits": "Кредити скидання",
            "reset_in_hours_minutes": "(скидання через %dг %dхв)",
            "reset_in_minutes": "(скидання через %dхв)",
            "reset_now": "скидання зараз",
            "duration_days_hours": "%dд %dг",
            "models_menu_title": "Вибір моделі CLI",
            "last_updated": "🕐 Оновлено: %@",
            "refresh_now": "🔄 Оновити зараз",
            "refreshing": "🔄 Оновлення...",
            "refresh_interval": "⏱ Інтервал оновлення",
            "minutes_short": "%d хв",
            "help_guide": "📖 Довідка та керівництво",
            "restart_app": "🚀 Перезапустити Codex App",
            "launch_at_login": "Запускати при вході",
            "quit": "Вихід",
            "remove_account": "🗑 Видалити акаунт...",
            "remove_account_title": "Видалити акаунт",
            "remove_account_confirm": "Ви впевнені, що хочете видалити %@ з Codex Monitor?",
            "remove_confirm_btn": "Видалити",
            "cancel_btn": "Скасувати",
            "add_account": "➕ Додати акаунт...",
            "add_account_title": "Додати акаунт Codex",
            "add_account_msg": "Введіть ідентифікатор акаунта (наприклад: personal, work, secondary):",
            "add_account_login_browser": "Увійти через браузер",
            "add_account_save_current": "Зберегти поточну сесію",
            "add_account_browser_prompt": "Відкриваємо браузер для входу. Будь ласка, завершіть авторизацію...",
            "add_account_success": "Акаунт '%@' успішно додано!",
            "add_account_saved_success": "Поточну сесію збережено як '%@'!",
            "add_account_failed_title": "Помилка додавання акаунта",
            "add_account_failed_desc": "Не вдалося додати акаунт. Спробуйте ще раз.",
            "switch_to_account": "Переключитися на цей акаунт",
            "cli_up_to_date": "✓ Codex CLI v%@ (актуальна версія)",
            "cli_update_available": "🚀 Оновити Codex CLI (до v%@)",
            "cli_not_found": "⚠️ Codex CLI не знайдено",
            "stack_percentages": "Компактний стек відсотків (2 рядки)",
            "auto_switch_on_limit": "🔄 Автоперемикання при вичерпанні ліміту",
            "auto_switch_business_only": "🏢 Автоперемикання тільки по бізнес-акаунтах",
            "auto_switch_business_priority": "⚡ Автоперемикання з пріоритетом бізнес-акаунтів"
        ],
        .de: [
            "menu_title": "OpenAI Codex Quota Monitor",
            "legend_circles": "5h Sprint · Wöchentliches Limit · Credits",
            "active_in_cli": "[AKTIV IN CLI]",
            "active_in_app": "[AKTIV IN APP]",
            "reserve_slot": "[RESERVE #%d]",
            "five_hour_sprint": "5h-Sprint:  %@  ",
            "weekly_limit": "Wöchentliches Limit:  %@  ",
            "reset_credits": "Reset-Guthaben",
            "reset_in_hours_minutes": "(Reset in %dh %dm)",
            "reset_in_minutes": "(Reset in %dm)",
            "reset_now": "Reset jetzt",
            "duration_days_hours": "%dT %dh",
            "models_menu_title": "CLI-Modell auswählen",
            "last_updated": "🕐 Aktualisiert: %@",
            "refresh_now": "🔄 Jetzt aktualisieren",
            "refreshing": "🔄 Aktualisieren...",
            "refresh_interval": "⏱ Aktualisierungsintervall",
            "minutes_short": "%d Min",
            "help_guide": "📖 Hilfe & Dokumentation",
            "restart_app": "🚀 Codex Desktop App neu starten",
            "launch_at_login": "Beim Login starten",
            "quit": "Beenden",
            "remove_account": "🗑 Konto entfernen...",
            "remove_account_title": "Konto entfernen",
            "remove_account_confirm": "Möchten Sie %@ wirklich aus Codex Monitor entfernen?",
            "remove_confirm_btn": "Entfernen",
            "cancel_btn": "Abbrechen",
            "add_account": "➕ Konto hinzufügen...",
            "switch_to_account": "Zu diesem Konto wechseln",
            "cli_up_to_date": "✓ Codex CLI v%@ (aktuell)",
            "cli_update_available": "🚀 Codex CLI aktualisieren (auf v%@)",
            "cli_not_found": "⚠️ Codex CLI nicht gefunden",
            "stack_percentages": "Kompakter Prozent-Stapel (2 Zeilen)",
            "auto_switch_on_limit": "🔄 Automatischer Wechsel bei Limit-Erschöpfung",
            "auto_switch_business_only": "🏢 Automatischer Wechsel nur für Business-Konten",
            "auto_switch_business_priority": "⚡ Automatischer Wechsel mit Business-Konto-Priorität"
        ],
        .fr: [
            "menu_title": "OpenAI Codex Quota Monitor",
            "legend_circles": "Sprint 5h · Limite Hebdo · Crédits",
            "active_in_cli": "[ACTIF DANS CLI]",
            "active_in_app": "[ACTIF DANS APP]",
            "reserve_slot": "[RÉSERVE #%d]",
            "five_hour_sprint": "Sprint 5h :  %@  ",
            "weekly_limit": "Limite hebdo :  %@  ",
            "reset_credits": "Crédits de réinitialisation",
            "reset_in_hours_minutes": "(réinit dans %dh %dm)",
            "reset_in_minutes": "(réinit dans %dm)",
            "reset_now": "réinitialisation maintenant",
            "duration_days_hours": "%dj %dh",
            "models_menu_title": "Choisir le modèle CLI",
            "last_updated": "🕐 Mis à jour : %@",
            "refresh_now": "🔄 Actualiser maintenant",
            "refreshing": "🔄 Actualisation...",
            "refresh_interval": "⏱ Intervalle d'actualisation",
            "minutes_short": "%d min",
            "help_guide": "📖 Aide et documentation",
            "restart_app": "🚀 Redémarrer l'application Codex",
            "launch_at_login": "Lancer au démarrage",
            "quit": "Quitter",
            "remove_account": "🗑 Supprimer le compte...",
            "remove_account_title": "Supprimer le compte",
            "remove_account_confirm": "Voulez-vous vraiment supprimer %@ de Codex Monitor ?",
            "remove_confirm_btn": "Supprimer",
            "cancel_btn": "Annuler",
            "add_account": "➕ Ajouter un compte...",
            "switch_to_account": "Basculer vers ce compte",
            "cli_up_to_date": "✓ Codex CLI v%@ (à jour)",
            "cli_update_available": "🚀 Mettre à jour Codex CLI (vers v%@)",
            "cli_not_found": "⚠️ Codex CLI introuvable",
            "stack_percentages": "Empiler les pourcentages (2 lignes)",
            "auto_switch_on_limit": "🔄 Basculement automatique en cas d'épuisement",
            "auto_switch_business_only": "🏢 Basculement automatique uniquement sur comptes Business",
            "auto_switch_business_priority": "⚡ Basculement automatique prioritaire sur comptes Business"
        ],
        .es: [
            "menu_title": "OpenAI Codex Quota Monitor",
            "legend_circles": "Sprint 5h · Límite Semanal · Créditos",
            "active_in_cli": "[ACTIVO EN CLI]",
            "active_in_app": "[ACTIVO EN APP]",
            "reserve_slot": "[RESERVA #%d]",
            "five_hour_sprint": "Sprint 5h:  %@  ",
            "weekly_limit": "Límite semanal:  %@  ",
            "reset_credits": "Créditos de restablecimiento",
            "reset_in_hours_minutes": "(reinicio en %dh %dm)",
            "reset_in_minutes": "(reinicio en %dm)",
            "reset_now": "reinicio ahora",
            "duration_days_hours": "%dd %dh",
            "models_menu_title": "Seleccionar modelo CLI",
            "last_updated": "🕐 Actualizado: %@",
            "refresh_now": "🔄 Actualizar ahora",
            "refreshing": "🔄 Actualizando...",
            "refresh_interval": "⏱ Intervalo de actualización",
            "minutes_short": "%d min",
            "help_guide": "📖 Ayuda y documentación",
            "restart_app": "🚀 Reiniciar aplicación Codex",
            "launch_at_login": "Iniciar al arrancar",
            "quit": "Salir",
            "remove_account": "🗑 Eliminar cuenta...",
            "remove_account_title": "Eliminar cuenta",
            "remove_account_confirm": "¿Seguro que deseas eliminar %@ de Codex Monitor?",
            "remove_confirm_btn": "Eliminar",
            "cancel_btn": "Cancelar",
            "add_account": "➕ Añadir cuenta...",
            "add_account_title": "Añadir cuenta de Codex",
            "add_account_msg": "Introduce el identificador de la cuenta (ej. personal, trabajo, equipo-2):",
            "add_account_login_browser": "Iniciar sesión en navegador",
            "add_account_save_current": "Guardar sesión actual",
            "add_account_browser_prompt": "Abriendo el navegador para iniciar sesión. Por favor completa la autenticación...",
            "add_account_success": "¡Cuenta '%@' añadida con éxito!",
            "add_account_saved_success": "¡Sesión actual guardada como '%@'!",
            "add_account_failed_title": "Error al añadir cuenta",
            "add_account_failed_desc": "No se pudo completar la configuración de la cuenta. Comprueba las credenciales e inténtalo de nuevo.",
            "switch_to_account": "Cambiar a esta cuenta",
            "cli_up_to_date": "✓ Codex CLI v%@ (al día)",
            "cli_update_available": "🚀 Actualizar Codex CLI (a v%@)",
            "cli_not_found": "⚠️ Codex CLI no encontrado",
            "stack_percentages": "Apilar porcentajes (2 filas)",
            "auto_switch_on_limit": "🔄 Cambio automático al agotar el límite",
            "auto_switch_business_only": "🏢 Cambio automático solo en cuentas de empresa",
            "auto_switch_business_priority": "⚡ Cambio automático con prioridad de cuentas de empresa"
        ],
        .it: [
            "menu_title": "OpenAI Codex Quota Monitor",
            "legend_circles": "Sprint 5h · Limite Settimanale · Crediti",
            "active_in_cli": "[ATTIVO IN CLI]",
            "active_in_app": "[ATTIVO IN APP]",
            "reserve_slot": "[RISERVA #%d]",
            "five_hour_sprint": "Sprint 5h:  %@  ",
            "weekly_limit": "Limite settimanale:  %@  ",
            "reset_credits": "Crediti di ripristino",
            "reset_in_hours_minutes": "(ripristino tra %dh %dm)",
            "reset_in_minutes": "(ripristino tra %dm)",
            "reset_now": "ripristino adesso",
            "duration_days_hours": "%dg %dh",
            "models_menu_title": "Seleziona modello CLI",
            "last_updated": "🕐 Aggiornato: %@",
            "refresh_now": "🔄 Aggiorna ora",
            "refreshing": "🔄 Aggiornamento...",
            "refresh_interval": "⏱ Intervallo di aggiornamento",
            "minutes_short": "%d min",
            "help_guide": "📖 Guida e documentazione",
            "restart_app": "🚀 Riavvia app desktop Codex",
            "launch_at_login": "Avvia al login",
            "quit": "Esci",
            "remove_account": "🗑 Rimuovi account...",
            "remove_account_title": "Rimuovi account",
            "remove_account_confirm": "Sei sicuro di voler rimuovere %@ da Codex Monitor?",
            "remove_confirm_btn": "Rimuovi",
            "cancel_btn": "Annulla",
            "add_account": "➕ Aggiungi account...",
            "switch_to_account": "Passa a questo account",
            "cli_up_to_date": "✓ Codex CLI v%@ (aggiornato)",
            "cli_update_available": "🚀 Aggiorna Codex CLI (a v%@)",
            "cli_not_found": "⚠️ Codex CLI non trovato",
            "stack_percentages": "Impila percentuali (2 righe)",
            "auto_switch_on_limit": "🔄 Cambio automatico all'esaurimento del limite",
            "auto_switch_business_only": "🏢 Cambio automatico solo account aziendali",
            "auto_switch_business_priority": "⚡ Cambio automatico con priorità account aziendali"
        ],
        .pt: [
            "menu_title": "OpenAI Codex Quota Monitor",
            "legend_circles": "Sprint 5h · Limite Semanal · Créditos",
            "active_in_cli": "[ATIVO NO CLI]",
            "active_in_app": "[ATIVO NO APP]",
            "reserve_slot": "[RESERVA #%d]",
            "five_hour_sprint": "Sprint 5h:  %@  ",
            "weekly_limit": "Limite semanal:  %@  ",
            "reset_credits": "Créditos de redefinição",
            "reset_in_hours_minutes": "(redefinição em %dh %dm)",
            "reset_in_minutes": "(redefinição em %dm)",
            "reset_now": "redefinição agora",
            "duration_days_hours": "%dd %dh",
            "models_menu_title": "Selecionar modelo CLI",
            "last_updated": "🕐 Atualizado: %@",
            "refresh_now": "🔄 Atualizar agora",
            "refreshing": "🔄 Atualizando...",
            "refresh_interval": "⏱ Intervalo de atualização",
            "minutes_short": "%d min",
            "help_guide": "📖 Ajuda e documentação",
            "restart_app": "🚀 Reiniciar app desktop Codex",
            "launch_at_login": "Iniciar no login",
            "quit": "Sair",
            "remove_account": "🗑 Remover conta...",
            "remove_account_title": "Remover conta",
            "remove_account_confirm": "Tem certeza de que deseja remover %@ do Codex Monitor?",
            "remove_confirm_btn": "Remover",
            "cancel_btn": "Cancelar",
            "add_account": "➕ Adicionar conta...",
            "switch_to_account": "Alternar para esta conta",
            "cli_up_to_date": "✓ Codex CLI v%@ (atualizado)",
            "cli_update_available": "🚀 Atualizar Codex CLI (para v%@)",
            "cli_not_found": "⚠️ Codex CLI não encontrado",
            "stack_percentages": "Empilhar percentagens (2 linhas)",
            "auto_switch_on_limit": "🔄 Alternância automática ao esgotar o limite",
            "auto_switch_business_only": "🏢 Alternância automática apenas contas empresariais",
            "auto_switch_business_priority": "⚡ Alternância automática com prioridade para contas de negócios"
        ],
        .pl: [
            "menu_title": "OpenAI Codex Quota Monitor",
            "legend_circles": "Sprint 5h · Limit Tygodniowy · Kredyty",
            "active_in_cli": "[AKTYWNY W CLI]",
            "active_in_app": "[AKTYWNY W APP]",
            "reserve_slot": "[REZERWA #%d]",
            "five_hour_sprint": "Sprint 5h:  %@  ",
            "weekly_limit": "Limit tygodniowy:  %@  ",
            "reset_credits": "Kredyty resetu",
            "reset_in_hours_minutes": "(reset za %dg %dm)",
            "reset_in_minutes": "(reset za %dm)",
            "reset_now": "reset teraz",
            "duration_days_hours": "%dd %dg",
            "models_menu_title": "Wybierz model CLI",
            "last_updated": "🕐 Zaktualizowano: %@",
            "refresh_now": "🔄 Odśwież teraz",
            "refreshing": "🔄 Odświeżanie...",
            "refresh_interval": "⏱ Interwał odświeżania",
            "minutes_short": "%d min",
            "help_guide": "📖 Pomoc i dokumentacja",
            "restart_app": "🚀 Uruchom ponownie aplikację Codex",
            "launch_at_login": "Uruchamiaj przy starcie",
            "quit": "Zakończ",
            "remove_account": "🗑 Usuń konto...",
            "remove_account_title": "Usuń konto",
            "remove_account_confirm": "Czy na pewno chcesz usunąć %@ z Codex Monitor?",
            "remove_confirm_btn": "Usuń",
            "cancel_btn": "Anuluj",
            "add_account": "➕ Dodaj konto...",
            "add_account_title": "Dodaj konto Codex",
            "add_account_msg": "Wprowadź identyfikator konta (np. personal, work, secondary):",
            "add_account_login_browser": "Zaloguj w przeglądarce",
            "add_account_save_current": "Zapisz bieżącą sesję",
            "add_account_browser_prompt": "Otwieranie przeglądarki w celu logowania. Ukończ autoryzację w przeglądarce...",
            "add_account_success": "Konto '%@' dodano pomyślnie!",
            "add_account_saved_success": "Bieżąca sesja została zapisana jako '%@'!",
            "add_account_failed_title": "Błąd dodawania konta",
            "add_account_failed_desc": "Nie udało się ukończyć konfiguracji konta. Sprawdź poświadczenia i spróbuj ponownie.",
            "switch_to_account": "Przełącz na to konto",
            "cli_up_to_date": "✓ Codex CLI v%@ (aktualny)",
            "cli_update_available": "🚀 Zaktualizuj Codex CLI (do v%@)",
            "cli_not_found": "⚠️ Nie znaleziono Codex CLI",
            "stack_percentages": "Kompaktowy stos procentów (2 wiersze)",
            "auto_switch_on_limit": "🔄 Automatyczne przełączanie po wyczerpaniu limitu",
            "auto_switch_business_only": "🏢 Automatyczne przełączanie tylko na konta biznesowe",
            "auto_switch_business_priority": "⚡ Automatyczne przełączanie z priorytetem kont biznesowych"
        ],
        .nl: [
            "menu_title": "OpenAI Codex Quota Monitor",
            "legend_circles": "5u Sprint · Wekelijks Limiet · Credits",
            "active_in_cli": "[ACTIEF IN CLI]",
            "active_in_app": "[ACTIEF IN APP]",
            "reserve_slot": "[RESERVE #%d]",
            "five_hour_sprint": "5u-sprint:  %@  ",
            "weekly_limit": "Wekelijks limiet:  %@  ",
            "reset_credits": "Reset-tegoeden",
            "reset_in_hours_minutes": "(reset over %du %dm)",
            "reset_in_minutes": "(reset over %dm)",
            "reset_now": "reset nu",
            "duration_days_hours": "%dd %du",
            "models_menu_title": "CLI-model selecteren",
            "last_updated": "🕐 Bijgewerkt: %@",
            "refresh_now": "🔄 Nu vernieuwen",
            "refreshing": "🔄 Vernieuwen...",
            "refresh_interval": "⏱ Vernieuwingsinterval",
            "minutes_short": "%d min",
            "help_guide": "📖 Hulp en documentatie",
            "restart_app": "🚀 Codex Desktop App herstarten",
            "launch_at_login": "Starten bij inloggen",
            "quit": "Afsluiten",
            "remove_account": "🗑 Account verwijderen...",
            "remove_account_title": "Account verwijderen",
            "remove_account_confirm": "Weet u zeker dat u %@ wilt verwijderen uit Codex Monitor?",
            "remove_confirm_btn": "Verwijderen",
            "cancel_btn": "Annuleren",
            "add_account": "➕ Account toevoegen...",
            "switch_to_account": "Overschakelen naar dit account",
            "cli_up_to_date": "✓ Codex CLI v%@ (up-to-date)",
            "cli_update_available": "🚀 Codex CLI bijwerken (naar v%@)",
            "cli_not_found": "⚠️ Codex CLI niet gevonden",
            "stack_percentages": "Compacte procentstapel (2 rijen)",
            "auto_switch_on_limit": "🔄 Automatisch overschakelen bij limietuitputting",
            "auto_switch_business_only": "🏢 Alleen automatisch overschakelen tussen zakelijke accounts",
            "auto_switch_business_priority": "⚡ Automatisch overschakelen met voorrang voor zakelijke accounts"
        ],
        .ja: [
            "menu_title": "OpenAI Codex Quota Monitor",
            "legend_circles": "上 = 5時間スプリント · 中央 = 週間上限 · 下 = リセットクレジット",
            "active_in_cli": "[CLIでアクティブ]",
            "active_in_app": "[APPでアクティブ]",
            "reserve_slot": "[待機 #%d]",
            "five_hour_sprint": "5時間スプリント:  %@  ",
            "weekly_limit": "週間上限:  %@  ",
            "reset_credits": "リセットクレジット",
            "reset_in_hours_minutes": "(%d時間%d分後にリセット)",
            "reset_in_minutes": "(%d分後にリセット)",
            "reset_soon": "(まもなくリセット)",
            "reset_now": "今すぐリセット",
            "hours_short": "%d時間",
            "hours_minutes_short": "%d時間%d分",
            "minutes_short": "%d分",
            "duration_days_hours": "%d日 %d時間",
            "duration_hours_minutes": "%d時間%d分",
            "duration_minutes": "%d分",
            "models_menu_title": "CLIモデル選択",
            "last_updated": "🕐 更新: %@",
            "refresh_now": "🔄 今すぐ更新",
            "refreshing": "🔄 更新中...",
            "refresh_interval": "⏱ 更新間隔",
            "help_guide": "📖 ヘルプとドキュメント",
            "restart_app": "🚀 Codex デスクトップアプリを再起動",
            "launch_at_login": "ログイン時に起動",
            "quit": "終了",
            "remove_account": "🗑 アカウントを削除...",
            "remove_account_title": "アカウントの削除",
            "remove_account_confirm": "本当に %@ を Codex Monitor から削除しますか？",
            "remove_confirm_btn": "削除",
            "cancel_btn": "キャンセル",
            "add_account": "➕ アカウントを追加...",
            "add_account_title": "Codex アカウントの追加",
            "add_account_msg": "アカウント識別子を入力してください (例: personal, work, secondary):",
            "add_account_login_browser": "ブラウザでログイン",
            "add_account_save_current": "現在のセッションを保存",
            "add_account_browser_prompt": "ログイン用ブラウザを開きます。ブラウザで認証を完了してください...",
            "add_account_success": "アカウント '%@' が正常に追加されました！",
            "add_account_saved_success": "現在のセッションが '%@' として保存されました！",
            "add_account_failed_title": "アカウント追加に失敗しました",
            "add_account_failed_desc": "アカウントのセットアップを完了できませんでした。認証情報を確認して再試行してください。",
            "switch_to_account": "このアカウントに切り替え",
            "cli_up_to_date": "✓ Codex CLI v%@ (最新)",
            "cli_update_available": "🚀 Codex CLI を更新 (v%@ へ)",
            "cli_not_found": "⚠️ Codex CLI が見つかりません",
            "rename_account": "✏️ ニックネームを編集...",
            "rename_account_title": "アカウントニックネームの編集",
            "rename_account_prompt": "%@ の表示ニックネームを入力:",
            "save_btn": "保存",
            "clear_btn": "クリア",
            "restart_app_on_switch": "🚀 切り替え時に Codex App を再起動",
            "stack_percentages": "パーセントの2行スタック表示",
            "auto_switch_on_limit": "🔄 制限到達時の自動切り替え",
            "auto_switch_business_only": "🏢 ビジネスアカウントのみ自動切り替え",
            "auto_switch_business_priority": "⚡ ビジネスアカウント優先で自動切り替え"
        ],
        .zhHans: [
            "menu_title": "OpenAI Codex Quota Monitor",
            "legend_circles": "顶部 = 5小时冲刺 · 中间 = 周额度 · 底部 = 重置点数",
            "active_in_cli": "[CLI 中活跃]",
            "active_in_app": "[APP 中活跃]",
            "reserve_slot": "[备用 #%d]",
            "five_hour_sprint": "5小时冲刺:  %@  ",
            "weekly_limit": "每周限额:  %@  ",
            "reset_credits": "重置点数",
            "reset_in_hours_minutes": "(%d小时%d分钟后重置)",
            "reset_in_minutes": "(%d分钟后重置)",
            "reset_soon": "(即将重置)",
            "reset_now": "立即重置",
            "hours_short": "%d小时",
            "hours_minutes_short": "%d小时%d分",
            "minutes_short": "%d分钟",
            "duration_days_hours": "%d天 %d小时",
            "duration_hours_minutes": "%d小时%d分",
            "duration_minutes": "%d分钟",
            "models_menu_title": "CLI 模型选择",
            "last_updated": "🕐 已更新: %@",
            "refresh_now": "🔄 立即刷新",
            "refreshing": "🔄 正在刷新...",
            "refresh_interval": "⏱ 刷新间隔",
            "help_guide": "📖 帮助与文档",
            "restart_app": "🚀 重启 Codex 桌面应用",
            "launch_at_login": "开机自启动",
            "quit": "退出",
            "remove_account": "🗑 移除账号...",
            "remove_account_title": "移除账号",
            "remove_account_confirm": "确定要从 Codex Monitor 中移除 %@ 吗？",
            "remove_confirm_btn": "移除",
            "cancel_btn": "取消",
            "add_account": "➕ 添加账号...",
            "add_account_title": "添加 Codex 账号",
            "add_account_msg": "请输入账号标识符 (例如: personal, work, secondary):",
            "add_account_login_browser": "通过浏览器登录",
            "add_account_save_current": "保存当前会话",
            "add_account_browser_prompt": "正在打开浏览器进行登录，请在浏览器中完成认证...",
            "add_account_success": "账号 '%@' 添加成功！",
            "add_account_saved_success": "当前会话已成功保存为 '%@'！",
            "add_account_failed_title": "添加账号失败",
            "add_account_failed_desc": "无法完成账号配置，请检查凭据后重试。",
            "switch_to_account": "切换到此账号",
            "cli_up_to_date": "✓ Codex CLI v%@ (已是最新)",
            "cli_update_available": "🚀 更新 Codex CLI (至 v%@)",
            "cli_not_found": "⚠️ 未找到 Codex CLI",
            "rename_account": "✏️ 编辑昵称...",
            "rename_account_title": "编辑账号昵称",
            "rename_account_prompt": "输入 %@ 的显示昵称:",
            "save_btn": "保存",
            "clear_btn": "清除",
            "restart_app_on_switch": "🚀 切换时自动重启 Codex App",
            "stack_percentages": "百分比紧凑堆叠 (双行)",
            "auto_switch_on_limit": "🔄 额度耗尽时自动切换",
            "auto_switch_business_only": "🏢 仅在企业/商业账号间自动切换",
            "auto_switch_business_priority": "⚡ 优先自动切换至企业账号"
        ],
        .vi: [
            "menu_title": "OpenAI Codex Quota Monitor",
            "legend_circles": "Trên = 5h Nước rút · Giữa = Hạn mức tuần · Dưới = Điểm đặt lại",
            "active_in_cli": "[HOẠT ĐỘNG TRONG CLI]",
            "active_in_app": "[HOẠT ĐỘNG TRONG APP]",
            "reserve_slot": "[DỰ PHÒNG #%d]",
            "five_hour_sprint": "5h nước rút:  %@  ",
            "weekly_limit": "Hạn mức tuần:  %@  ",
            "reset_credits": "Điểm đặt lại",
            "reset_in_hours_minutes": "(đặt lại sau %dh %dm)",
            "reset_in_minutes": "(đặt lại sau %dm)",
            "reset_soon": "(sắp đặt lại)",
            "reset_now": "đặt lại ngay",
            "hours_short": "%dh",
            "hours_minutes_short": "%dh %dm",
            "minutes_short": "%d phút",
            "duration_days_hours": "%d ngày %d giờ",
            "duration_hours_minutes": "%dh %dm",
            "duration_minutes": "%dm",
            "models_menu_title": "Chọn mô hình CLI",
            "last_updated": "🕐 Cập nhật: %@",
            "refresh_now": "🔄 Làm mới ngay",
            "refreshing": "🔄 Đang làm mới...",
            "refresh_interval": "⏱ Khoảng thời gian làm mới",
            "help_guide": "📖 Hướng dẫn & Tài liệu",
            "restart_app": "🚀 Khởi động lại ứng dụng Codex",
            "launch_at_login": "Khởi động cùng hệ thống",
            "quit": "Thoát",
            "remove_account": "🗑 Xóa tài khoản...",
            "remove_account_title": "Xóa tài khoản",
            "remove_account_confirm": "Bạn có chắc chắn muốn xóa %@ khỏi Codex Monitor?",
            "remove_confirm_btn": "Xóa",
            "cancel_btn": "Hủy",
            "add_account": "➕ Thêm tài khoản...",
            "add_account_title": "Thêm tài khoản Codex",
            "add_account_msg": "Nhập định danh tài khoản (ví dụ: personal, work, secondary):",
            "add_account_login_browser": "Đăng nhập qua trình duyệt",
            "add_account_save_current": "Lưu phiên hiện tại",
            "add_account_browser_prompt": "Đang mở trình duyệt để đăng nhập. Vui lòng hoàn tất xác thực trong trình duyệt...",
            "add_account_success": "Tài khoản '%@' đã được thêm thành công!",
            "add_account_saved_success": "Phiên hiện tại đã được lưu thành '%@'!",
            "add_account_failed_title": "Thêm tài khoản thất bại",
            "add_account_failed_desc": "Không thể hoàn tất thiết lập tài khoản. Vui lòng kiểm tra thông tin và thử lại.",
            "switch_to_account": "Chuyển sang tài khoản này",
            "cli_up_to_date": "✓ Codex CLI v%@ (mới nhất)",
            "cli_update_available": "🚀 Cập nhật Codex CLI (lên v%@)",
            "cli_not_found": "⚠️ Không tìm thấy Codex CLI",
            "rename_account": "✏️ Sửa biệt danh...",
            "rename_account_title": "Sửa biệt danh tài khoản",
            "rename_account_prompt": "Nhập biệt danh hiển thị cho %@:",
            "save_btn": "Lưu",
            "clear_btn": "Xóa",
            "restart_app_on_switch": "🚀 Khởi động lại Codex App khi chuyển",
            "stack_percentages": "Xếp chồng phần trăm (2 dòng)",
            "auto_switch_on_limit": "🔄 Tự động chuyển khi hết hạn mức",
            "auto_switch_business_only": "🏢 Chỉ tự động chuyển tài khoản doanh nghiệp",
            "auto_switch_business_priority": "⚡ Tự động chuyển ưu tiên tài khoản doanh nghiệp"
        ]
    ]
}

public enum L10n {
    public static func tr(_ key: String) -> String {
        return LocalizationManager.shared.string(forKey: key)
    }

    public static var menuTitle: String { tr("menu_title") }
    public static var legendCircles: String { tr("legend_circles") }
    public static var activeInCli: String { tr("active_in_cli") }
    public static var activeInApp: String { tr("active_in_app") }
    public static func reserveSlot(index: Int) -> String {
        String(format: tr("reserve_slot"), index)
    }
    public static var fiveHourSprint: String { tr("five_hour_sprint") }
    public static var weeklyLimit: String { tr("weekly_limit") }
    public static var resetCredits: String { tr("reset_credits") }
    public static func resetIn(hours: Int, minutes: Int) -> String {
        String(format: tr("reset_in_hours_minutes"), hours, minutes)
    }
    public static func resetIn(minutes: Int) -> String {
        String(format: tr("reset_in_minutes"), minutes)
    }
    public static var resetNow: String { tr("reset_now") }
    public static func duration(days: Int, hours: Int) -> String {
        let fmt = tr("duration_days_hours")
        if fmt != "duration_days_hours" {
            return String(format: fmt, days, hours)
        }
        return String(format: "%dd %dh", days, hours)
    }
    public static func durationHoursMinutes(hours: Int, minutes: Int) -> String {
        let fmt = tr("duration_hours_minutes")
        if fmt != "duration_hours_minutes" {
            return String(format: fmt, hours, minutes)
        }
        return String(format: "%dh %02dm", hours, minutes)
    }
    public static func durationMinutes(minutes: Int) -> String {
        let fmt = tr("duration_minutes")
        if fmt != "duration_minutes" {
            return String(format: fmt, minutes)
        }
        return String(format: "%dm", minutes)
    }
    public static var modelsMenuTitle: String { tr("models_menu_title") }
    public static func lastUpdated(time: String) -> String {
        String(format: tr("last_updated"), time)
    }
    public static var refreshNow: String { tr("refresh_now") }
    public static var refreshing: String { tr("refreshing") }
    public static var refreshInterval: String { tr("refresh_interval") }
    public static func minutesShort(count: Int) -> String {
        String(format: tr("minutes_short"), count)
    }
    public static var helpGuide: String { tr("help_guide") }
    public static var restartApp: String { tr("restart_app") }
    public static var launchAtLogin: String { tr("launch_at_login") }
    public static var quit: String { tr("quit") }
    public static var removeAccount: String { tr("remove_account") }
    public static var removeAccountTitle: String { tr("remove_account_title") }
    public static func removeAccountConfirm(email: String) -> String {
        String(format: tr("remove_account_confirm"), email)
    }
    public static var removeConfirmBtn: String { tr("remove_confirm_btn") }
    public static var cancelBtn: String { tr("cancel_btn") }
    public static var addAccount: String { tr("add_account") }
    public static var addAccountTitle: String { tr("add_account_title") }
    public static var addAccountMsg: String { tr("add_account_msg") }
    public static var addAccountLoginBrowserBtn: String { tr("add_account_login_browser") }
    public static var addAccountSaveCurrentBtn: String { tr("add_account_save_current") }
    public static func addAccountBrowserPrompt(id: String) -> String {
        String(format: tr("add_account_browser_prompt"), id)
    }
    public static func addAccountSuccess(id: String) -> String {
        String(format: tr("add_account_success"), id)
    }
    public static func addAccountSavedSuccess(id: String) -> String {
        String(format: tr("add_account_saved_success"), id)
    }
    public static var addAccountFailedTitle: String { tr("add_account_failed_title") }
    public static var addAccountFailedDesc: String { tr("add_account_failed_desc") }
    public static var switchToAccount: String { tr("switch_to_account") }
    public static func cliUpToDate(version: String) -> String {
        String(format: tr("cli_up_to_date"), version)
    }
    public static func cliUpdateAvailable(version: String) -> String {
        String(format: tr("cli_update_available"), version)
    }
    public static var cliNotFound: String { tr("cli_not_found") }
    public static var renameAccount: String { tr("rename_account") }
    public static var renameAccountTitle: String { tr("rename_account_title") }
    public static func renameAccountPrompt(email: String) -> String {
        String(format: tr("rename_account_prompt"), email)
    }
    public static var saveBtn: String { tr("save_btn") }
    public static var clearBtn: String { tr("clear_btn") }
    public static var restartAppOnSwitch: String { tr("restart_app_on_switch") }
    public static var autoSwitchOnLimit: String { tr("auto_switch_on_limit") }
    public static var autoSwitchBusinessOnly: String { tr("auto_switch_business_only") }
    public static var autoSwitchBusinessPriority: String { tr("auto_switch_business_priority") }
    public static var stackPercentages: String { tr("stack_percentages") }
}
