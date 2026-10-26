// arka-wheel: the ArkaOS format wheel. Runs hidden for the whole session; the
// "Show the format wheel" shortcut (Meta+Alt+C by default, changeable in System
// Settings > Shortcuts) shows a ring of formats at the cursor while you drag a
// file. Drop the file on a format and arka-convert makes the copy next to the
// original. `arka-wheel --files FILE...` (the Dolphin right-click entry) shows
// the same ring for clicking. "Show the edit wheel" (Meta+Alt+E, or `--edit`)
// shows a ring of edits instead: crop, black out, trim and so on. All the work
// is done by arka-convert, or by arka-edit when the tool needs a window.

#include <QAction>
#include <QDBusConnection>
#include <QDBusConnectionInterface>
#include <QDBusMessage>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QGuiApplication>
#include <QIcon>
#include <QPalette>
#include <QProcess>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickImageProvider>
#include <QQuickWindow>
#include <QMouseEvent>
#include <QRegion>
#include <QTimer>
#include <QUrl>

#include <KGlobalAccel>
#include <KWindowEffects>
#include <LayerShellQt/Window>

static const QString Service = QStringLiteral("org.arkaos.Wheel");
static const QString Path = QStringLiteral("/Wheel");

// The formats each kind of file can become, in wheel order. Every entry must
// also be a target in arka-convert, which checks the request again anyway.
static QStringList formatsForFile(const QString &path)
{
    static const QStringList image{"png", "jpg", "jpeg", "webp", "gif", "bmp", "tif", "tiff"};
    static const QStringList audio{"mp3", "wav", "flac", "ogg", "oga", "opus", "m4a", "aac"};
    static const QStringList video{"mp4", "mkv", "webm", "mov", "avi", "m4v", "ogv"};
    QString ext = QFileInfo(path).suffix().toLower();
    QStringList out;
    if (image.contains(ext))
        out = {"png", "jpg", "webp", "pdf"};
    else if (audio.contains(ext))
        out = {"mp3", "wav", "flac", "ogg", "m4a"};
    else if (video.contains(ext))
        out = {"mp4", "mkv", "webm", "mp3"};
    else if (ext == "pdf")
        out = {"png", "smaller-pdf"};
    out.removeAll(ext == "jpeg" ? QStringLiteral("jpg") : ext); // not the format it already is
    return out;
}

// The edits each kind of file offers, in wheel order. "rotate-left" and the
// like are run by arka-convert directly; the rest open arka-edit.
static QStringList toolsForFile(const QString &path)
{
    static const QStringList image{"png", "jpg", "jpeg", "webp", "gif", "bmp", "tif", "tiff"};
    static const QStringList trimmable{"mp3", "wav", "flac", "ogg", "opus", "m4a", "mp4", "mkv", "webm"};
    const QString ext = QFileInfo(path).suffix().toLower();
    if (image.contains(ext))
        return {"crop", "redact", "background", "rotate-left", "rotate-right"};
    if (ext == "pdf")
        return {"split"};
    if (trimmable.contains(ext))
        return {"trim"};
    return {};
}

class IconProvider : public QQuickImageProvider
{
public:
    IconProvider() : QQuickImageProvider(QQuickImageProvider::Pixmap) {}
    QPixmap requestPixmap(const QString &id, QSize *size, const QSize &requested) override
    {
        const QString generic = id.section('-', 0, 0) + QStringLiteral("-x-generic");
        QIcon icon = QIcon::hasThemeIcon(id) ? QIcon::fromTheme(id) : QIcon::fromTheme(generic);
        const int side = requested.width() > 0 ? requested.width() : 48;
        QPixmap pm = icon.pixmap(side, side);
        if (size)
            *size = pm.size();
        return pm;
    }
};

class Wheel : public QObject
{
    Q_OBJECT
    Q_CLASSINFO("D-Bus Interface", "org.arkaos.Wheel")
    Q_PROPERTY(QColor accent READ accent CONSTANT)

public:
    QWindow *window = nullptr;

    QColor accent() const { return QGuiApplication::palette().color(QPalette::Highlight); }

    Q_INVOKABLE bool dark() const
    {
        QFile f(QDir::homePath() + QStringLiteral("/.config/arka-look"));
        if (f.open(QIODevice::ReadOnly))
            return f.readAll().trimmed() != "light";
        return true;
    }

    Q_INVOKABLE QStringList formatsFor(const QList<QUrl> &urls) const
    {
        QStringList common;
        bool first = true;
        for (const QUrl &u : urls) {
            if (!u.isLocalFile())
                return {};
            const QStringList f = formatsForFile(u.toLocalFile());
            if (first) {
                common = f;
                first = false;
            } else {
                QStringList keep;
                for (const QString &x : common)
                    if (f.contains(x))
                        keep << x;
                common = keep;
            }
        }
        return common;
    }

    // The edits that suit every one of these files. Two or more PDFs can be merged;
    // everything else works on one file at a time.
    Q_INVOKABLE QStringList toolsFor(const QList<QUrl> &urls) const
    {
        if (urls.isEmpty())
            return {};
        for (const QUrl &u : urls)
            if (!u.isLocalFile())
                return {};
        if (urls.size() > 1) {
            for (const QUrl &u : urls)
                if (QFileInfo(u.toLocalFile()).suffix().toLower() != QLatin1String("pdf"))
                    return {};
            return {"merge"};
        }
        return toolsForFile(urls.first().toLocalFile());
    }

    Q_INVOKABLE void runTool(const QString &tool, const QList<QUrl> &urls) const
    {
        QStringList files;
        for (const QUrl &u : urls)
            if (u.isLocalFile())
                files << u.toLocalFile();
        if (files.isEmpty())
            return;
        if (tool == QLatin1String("crop") || tool == QLatin1String("redact") || tool == QLatin1String("background")
            || tool == QLatin1String("trim"))
            QProcess::startDetached(QStringLiteral("arka-edit"), {tool, files.first()});
        else if (tool == QLatin1String("rotate-left"))
            QProcess::startDetached(QStringLiteral("arka-convert"), {"rotate", files.first(), "left"});
        else if (tool == QLatin1String("rotate-right"))
            QProcess::startDetached(QStringLiteral("arka-convert"), {"rotate", files.first(), "right"});
        else if (tool == QLatin1String("split"))
            QProcess::startDetached(QStringLiteral("arka-convert"), {"split-pdf", files.first()});
        else if (tool == QLatin1String("merge"))
            QProcess::startDetached(QStringLiteral("arka-convert"), QStringList{"merge-pdf"} + files);
    }

    Q_INVOKABLE void convert(const QString &format, const QList<QUrl> &urls) const
    {
        QStringList args{format};
        for (const QUrl &u : urls)
            if (u.isLocalFile())
                args << u.toLocalFile();
        if (args.size() > 1)
            QProcess::startDetached(QStringLiteral("arka-convert"), args);
    }

    // Blur only the ring itself, like the dock: KWin blurs what is behind it.
    Q_INVOKABLE void setBlur(qreal cx, qreal cy, qreal outer, qreal inner) const
    {
        if (!window)
            return;
        QRegion ring(QRect(cx - outer, cy - outer, 2 * outer, 2 * outer), QRegion::Ellipse);
        ring -= QRegion(QRect(cx - inner, cy - inner, 2 * inner, 2 * inner), QRegion::Ellipse);
        KWindowEffects::enableBlurBehind(window, true, ring);
    }

    Q_INVOKABLE void clearBlur() const
    {
        if (window)
            KWindowEffects::enableBlurBehind(window, false);
    }

public Q_SLOTS:
    Q_SCRIPTABLE void ShowFiles(const QStringList &paths) { showPaths(paths, false); }
    Q_SCRIPTABLE void ShowTools(const QStringList &paths) { showPaths(paths, true); }
    void showPaths(const QStringList &paths, bool edit)
    {
        QList<QUrl> urls;
        for (const QString &p : paths)
            urls << QUrl::fromLocalFile(QFileInfo(p).absoluteFilePath());
        Q_EMIT opened(urls, edit);
    }
    void showForDrag() { Q_EMIT opened({}, false); }
    void showEditForDrag() { Q_EMIT opened({}, true); }

Q_SIGNALS:
    // Empty urls: wait for a dragged file. Otherwise: click mode for these files.
    // edit: show the edits instead of the formats.
    void opened(const QList<QUrl> &urls, bool edit);
};

int main(int argc, char **argv)
{
    // A flat ring needs no GPU: software drawing keeps the waiting process at
    // about a quarter of the memory (measured 24 MB PSS against 146 MB RSS).
    if (qEnvironmentVariableIsEmpty("QT_QUICK_BACKEND"))
        qputenv("QT_QUICK_BACKEND", "software");
    QGuiApplication app(argc, argv);
    app.setApplicationName(QStringLiteral("arka-wheel"));
    app.setDesktopFileName(QStringLiteral("org.arkaos.wheel"));
    app.setQuitOnLastWindowClosed(false);

    QStringList files = app.arguments().mid(1);
    const bool editMode = !files.isEmpty() && files.first() == QLatin1String("--edit");
    if (editMode)
        files.removeFirst();
    const bool clickMode = !files.isEmpty() && files.first() == QLatin1String("--files");
    if (clickMode)
        files.removeFirst();

    auto bus = QDBusConnection::sessionBus();
    if (bus.interface()->isServiceRegistered(Service)) {
        // Already running for this session: hand over the files, or do nothing.
        if (clickMode) {
            auto msg = QDBusMessage::createMethodCall(Service, Path, Service,
                                                      editMode ? QStringLiteral("ShowTools") : QStringLiteral("ShowFiles"));
            msg << files;
            bus.call(msg);
        }
        return 0;
    }

    Wheel wheel;
    QQmlApplicationEngine engine;
    engine.addImageProvider(QStringLiteral("icon"), new IconProvider);
    engine.rootContext()->setContextProperty(QStringLiteral("wheel"), &wheel);
    engine.load(QUrl(QStringLiteral("qrc:/Wheel.qml")));
    auto *window = engine.rootObjects().isEmpty() ? nullptr
                                                  : qobject_cast<QQuickWindow *>(engine.rootObjects().first());
    if (!window)
        return 1;
    wheel.window = window;

    // For testing without a compositor: size the window, then save a picture of it and quit.
    const QString shot = qEnvironmentVariable("ARKA_WHEEL_SCREENSHOT");
    if (!shot.isEmpty()) {
        window->resize(1000, 700);
        // ARKA_WHEEL_EVENTS="m X Y;..." moves the pointer (window coordinates) before the picture is taken.
        int at = 600;
        for (const QString &e : qEnvironmentVariable("ARKA_WHEEL_EVENTS").split(';', Qt::SkipEmptyParts)) {
            const QStringList f = e.trimmed().split(' ', Qt::SkipEmptyParts);
            if (f.size() != 3)
                continue;
            QTimer::singleShot(at += 50, window, [window, f] {
                const QPointF p(f[1].toDouble(), f[2].toDouble());
                QMouseEvent ev(QEvent::MouseMove, p, p, window->mapToGlobal(p), Qt::NoButton, Qt::NoButton, Qt::NoModifier);
                QCoreApplication::sendEvent(window, &ev);
            });
        }
        QTimer::singleShot(at + 900, window, [window, shot] {
            window->grabWindow().save(shot);
            QGuiApplication::quit();
        });
    }

    auto *layer = LayerShellQt::Window::get(window);
    layer->setLayer(LayerShellQt::Window::LayerOverlay);
    layer->setAnchors(LayerShellQt::Window::Anchors(LayerShellQt::Window::AnchorTop | LayerShellQt::Window::AnchorBottom
                                                    | LayerShellQt::Window::AnchorLeft | LayerShellQt::Window::AnchorRight));
    layer->setExclusiveZone(-1);
    layer->setKeyboardInteractivity(LayerShellQt::Window::KeyboardInteractivityOnDemand);
    layer->setScope(QStringLiteral("arka-wheel"));
    layer->setWantsToBeOnActiveScreen(true); // the screen you are working on, not always the first

    bus.registerObject(Path, &wheel, QDBusConnection::ExportScriptableSlots);
    bus.registerService(Service);

    auto *action = new QAction(&app);
    action->setObjectName(QStringLiteral("show-wheel"));
    action->setText(QStringLiteral("Show the format wheel"));
    action->setProperty("componentName", QStringLiteral("arka-wheel"));
    action->setProperty("componentDisplayName", QStringLiteral("ArkaOS Format Wheel"));
    const QList<QKeySequence> keys{QKeySequence(Qt::META | Qt::ALT | Qt::Key_C)};
    KGlobalAccel::self()->setDefaultShortcut(action, keys);
    KGlobalAccel::self()->setShortcut(action, keys); // keeps a shortcut the user changed
    QObject::connect(action, &QAction::triggered, &wheel, &Wheel::showForDrag);

    auto *editAction = new QAction(&app);
    editAction->setObjectName(QStringLiteral("show-edit-wheel"));
    editAction->setText(QStringLiteral("Show the edit wheel"));
    editAction->setProperty("componentName", QStringLiteral("arka-wheel"));
    editAction->setProperty("componentDisplayName", QStringLiteral("ArkaOS Format Wheel"));
    const QList<QKeySequence> editKeys{QKeySequence(Qt::META | Qt::ALT | Qt::Key_E)};
    KGlobalAccel::self()->setDefaultShortcut(editAction, editKeys);
    KGlobalAccel::self()->setShortcut(editAction, editKeys);
    QObject::connect(editAction, &QAction::triggered, &wheel, &Wheel::showEditForDrag);

    if (clickMode)
        wheel.showPaths(files, editMode);
    return app.exec();
}

#include "main.moc"
