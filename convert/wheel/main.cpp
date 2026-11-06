// arka-wheel: the ArkaOS format wheel. Runs hidden for the whole session; the
// "Show the format wheel" shortcut (Meta+Alt+C by default, changeable in System
// Settings > Shortcuts) shows a ring of formats at the cursor while you drag a
// file. Drop the file on a format and arka-convert makes the copy next to the
// original. `arka-wheel --files FILE...` (the Dolphin right-click entry) shows
// the same ring for clicking. "Show the edit wheel" (Meta+Alt+E, or `--edit`)
// shows a ring of edits instead: crop, black out, trim and so on. All the work
// is done by arka-convert, or by arka-edit when the tool needs a window.

#include <cmath>
#include <algorithm>
#include <QAction>
#include <QCollator>
#include <QDBusConnection>
#include <QDBusConnectionInterface>
#include <QDBusMessage>
#include <QDir>
#include <QFile>
#include <QFont>
#include <QFontDatabase>
#include <QFileInfo>
#include <QGuiApplication>
#include <QIcon>
#include <QLocale>
#include <QPalette>
#include <QProcess>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickImageProvider>
#include <QQuickWindow>
#include <QMouseEvent>
#include <QRegion>
#include <QStandardPaths>
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
    static const QStringList image{"png", "jpg", "jpeg", "webp", "gif", "bmp", "tif", "tiff", "avif"};
    static const QStringList audio{"mp3", "wav", "flac", "ogg", "oga", "opus", "m4a", "aac"};
    static const QStringList video{"mp4", "mkv", "webm", "mov", "avi", "m4v", "ogv"};
    static const QStringList office{"doc", "docx", "odt", "rtf", "xls", "xlsx", "ods", "ppt", "pptx", "odp"};
    QString ext = QFileInfo(path).suffix().toLower();
    QStringList out;
    if (image.contains(ext))
        out = {"png", "jpg", "webp", "avif", "pdf", "smaller-image", "text", "tiff", "bmp", "zip"};
    else if (audio.contains(ext))
        out = {"mp3", "smaller-audio", "wav", "flac", "ogg", "m4a", "zip"};
    else if (video.contains(ext))
        out = {"mp4", "smaller-video", "mkv", "webm", "mp3", "zip"};
    else if (ext == "pdf")
        out = {"png", "smaller-pdf", "docx", "text", "zip"};
    else if (office.contains(ext))
        out = {"pdf", "zip"};
    else if (QFileInfo(path).isFile())
        out = {"zip"}; // any other file can still be put in a ZIP
    out.removeAll(ext == "jpeg" ? QStringLiteral("jpg") : ext == "tif" ? QStringLiteral("tiff") : ext); // not the format it already is
    // Only these pictures can be made smaller; animated and old formats are left alone.
    if (ext != "png" && ext != "jpg" && ext != "jpeg" && ext != "webp")
        out.removeAll(QStringLiteral("smaller-image"));
    return out;
}

// The edits each kind of file offers, in wheel order: the first five are on the ring, the
// rest behind "More". Tools that need choices open arka-edit; rotate, split pages, join,
// smaller and clean are run by arka-convert directly ("smaller" asks how much first).
static QStringList toolsForFile(const QString &path)
{
    static const QStringList image{"png", "jpg", "jpeg", "webp", "gif", "bmp", "tif", "tiff", "avif"};
    static const QStringList audio{"mp3", "wav", "flac", "ogg", "oga", "opus", "m4a", "aac"};
    static const QStringList video{"mp4", "mkv", "webm", "mov", "avi", "m4v", "ogv"};
    static const QStringList trimmable{"mp3", "wav", "flac", "ogg", "opus", "m4a", "mp4", "mkv", "webm"};
    // The formats "Remove hidden details" (mat2) handles; the same list as CLEAN_EXT in arka-convert.
    static const QStringList cleanable{"png", "jpg", "jpeg", "webp", "gif", "bmp", "tif", "tiff", "pdf", "mp3",
                                       "flac", "ogg", "oga", "opus", "wav", "mp4", "m4v", "avi"};
    const QString ext = QFileInfo(path).suffix().toLower();
    QStringList out;
    if (image.contains(ext)) {
        out = {"crop", "annotate", "redact", "hide-private", "smaller", "background", "clean", "rotate-left",
               "rotate-right"};
        if (ext != "png" && ext != "jpg" && ext != "jpeg" && ext != "webp")
            out.removeAll(QStringLiteral("smaller"));   // animated and old formats are left alone
    } else if (ext == "pdf") {
        out = {"split", "smaller", "clean"};
    } else if (audio.contains(ext)) {
        out = {"trim", "split-media", "bleep", "smaller", "clean"};
    } else if (video.contains(ext)) {
        out = {"trim", "crop", "redact", "bleep", "smaller", "split-media", "clean"};
    }
    if (!trimmable.contains(ext))
        out.removeAll(QStringLiteral("trim"));
    if (!cleanable.contains(ext))
        out.removeAll(QStringLiteral("clean"));
    return out;
}

static QString kindOf(const QString &path)
{
    static const QStringList audio{"mp3", "wav", "flac", "ogg", "oga", "opus", "m4a", "aac"};
    static const QStringList video{"mp4", "mkv", "webm", "mov", "avi", "m4v", "ogv"};
    const QString ext = QFileInfo(path).suffix().toLower();
    static const QStringList image{"png", "jpg", "jpeg", "webp", "gif", "bmp", "tif", "tiff", "avif"};
    return ext == QLatin1String("pdf") ? QStringLiteral("pdf")
         : image.contains(ext)         ? QStringLiteral("image")
         : audio.contains(ext)         ? QStringLiteral("audio")
         : video.contains(ext)         ? QStringLiteral("video")
                                       : QString();
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

    Q_INVOKABLE bool hasIcons() const { return true; }   // the "icon" image provider is registered

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
        // Two or more files and folders can always go into ONE ZIP, whatever they are, so this
        // does not depend on what the others share. It comes first, so it keeps the same place
        // on the ring each time.
        if (urls.size() >= 2) {
            bool all = true;
            for (const QUrl &u : urls)
                all = all && QFileInfo::exists(u.toLocalFile());
            if (all)
                common.prepend(QStringLiteral("zip-all"));
        }
        return common;
    }

    // The edits that suit every one of these files. Two or more PDFs can be merged, and two
    // or more sounds (or videos) joined; everything else works on one file at a time.
    Q_INVOKABLE QStringList toolsFor(const QList<QUrl> &urls) const
    {
        if (urls.isEmpty())
            return {};
        for (const QUrl &u : urls)
            if (!u.isLocalFile())
                return {};
        if (urls.size() > 1) {
            // Several files of one kind: put them together, and the edits that work file by file.
            const QString kind = kindOf(urls.first().toLocalFile());
            bool allSmaller = true, allClean = true;
            for (const QUrl &u : urls) {
                const QString path = u.toLocalFile();
                if (kindOf(path) != kind)
                    return {};
                const QStringList one = toolsForFile(path);
                allSmaller = allSmaller && one.contains(QStringLiteral("smaller"));
                allClean = allClean && one.contains(QStringLiteral("clean"));
            }
            QStringList multi;
            if (kind == QLatin1String("pdf"))
                multi << QStringLiteral("merge");
            else if (kind == QLatin1String("audio") || kind == QLatin1String("video"))
                multi << QStringLiteral("join");
            if (allSmaller)
                multi << QStringLiteral("smaller");
            if (allClean)
                multi << QStringLiteral("clean");
            return multi;
        }
        return toolsForFile(urls.first().toLocalFile());
    }

    // Bytes of all the files together; 0 when any of them is not a plain file.
    Q_INVOKABLE qint64 totalBytes(const QList<QUrl> &urls) const
    {
        qint64 total = 0;
        for (const QUrl &u : urls) {
            const QFileInfo fi(u.toLocalFile());
            if (!u.isLocalFile() || !fi.isFile())
                return 0;
            total += fi.size();
        }
        return total;
    }

    // How big the files are together, in the units Dolphin shows ("1.2 MiB"). Empty for folders.
    Q_INVOKABLE QString sizeText(const QList<QUrl> &urls) const
    {
        qint64 total = 0;
        for (const QUrl &u : urls) {
            const QFileInfo fi(u.toLocalFile());
            if (!u.isLocalFile() || !fi.isFile())
                return {};
            total += fi.size();
        }
        return urls.isEmpty() ? QString() : QLocale().formattedDataSize(total, 1, QLocale::DataSizeIecFormat);
    }

    Q_INVOKABLE void runTool(const QString &tool, const QList<QUrl> &urls) const
    {
        QStringList files;
        for (const QUrl &u : urls)
            if (u.isLocalFile())
                files << u.toLocalFile();
        if (files.isEmpty())
            return;
        static const QStringList windowed{"crop", "redact", "annotate", "background", "trim", "bleep"};
        if (windowed.contains(tool))
            QProcess::startDetached(QStringLiteral("arka-edit"), {tool, files.first()});
        else if (tool == QLatin1String("hide-private"))
            QProcess::startDetached(QStringLiteral("arka-edit"), {QStringLiteral("redact"), files.first(),
                                                                  QStringLiteral("--find-private")});
        else if (tool == QLatin1String("split-media"))
            QProcess::startDetached(QStringLiteral("arka-edit"), {QStringLiteral("split"), files.first()});
        else if (tool == QLatin1String("join")) {
            // In name order, as the file manager lists them: "clip 2" before "clip 10".
            QCollator order;
            order.setNumericMode(true);
            std::sort(files.begin(), files.end(), [&order](const QString &a, const QString &b) {
                return order.compare(QFileInfo(a).fileName(), QFileInfo(b).fileName()) < 0;
            });
            QProcess::startDetached(QStringLiteral("arka-convert"), QStringList{"join-media"} + files);
        } else if (tool == QLatin1String("clean"))
            QProcess::startDetached(QStringLiteral("arka-convert"), QStringList{"clean"} + files);
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
        // Both outlines are octagons with flat sides facing up, down, left and right.
        auto octagon = [cx, cy](qreal radius) {
            QPolygon poly;
            for (int q = 0; q < 8; ++q) {
                const qreal angle = (22.5 + q * 45) * M_PI / 180;
                poly << QPoint(qRound(cx + radius * std::cos(angle)), qRound(cy + radius * std::sin(angle)));
            }
            return QRegion(poly);
        };
        QRegion ring = octagon(outer);
        ring -= octagon(inner);
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
    // The type ArkaOS's own apps use, when installed (packaged as "Inter" or "Inter Variable").
    for (const QString &family : {QStringLiteral("Inter"), QStringLiteral("Inter Variable")}) {
        if (QFontDatabase::hasFamily(family)) {
            QFont f(family);
            f.setPointSizeF(QGuiApplication::font().pointSizeF());
            QGuiApplication::setFont(f);
            break;
        }
    }

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
    // The slot icons come from the desktop's icon theme. Without one (a bare session), use Breeze
    // in the matching shade, so the icons never silently disappear.
    QStringList iconDirs = QIcon::themeSearchPaths();
    for (const QString &dir : QStandardPaths::locateAll(QStandardPaths::GenericDataLocation, QStringLiteral("icons"),
                                                         QStandardPaths::LocateDirectory))
        if (!iconDirs.contains(dir))
            iconDirs << dir;
    QIcon::setThemeSearchPaths(iconDirs);
    if (QIcon::themeName().isEmpty() || QIcon::themeName() == QLatin1String("hicolor"))
        QIcon::setThemeName(wheel.dark() ? QStringLiteral("breeze-dark") : QStringLiteral("breeze"));
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

    // Meta+/ opens the keyboard shortcuts page. That page keeps a single window, so pressing
    // the keys again raises it instead of opening a second one. It is its own entry in
    // System Settings > Shortcuts, so it is not hidden under "Format Wheel".
    auto *keysAction = new QAction(&app);
    keysAction->setObjectName(QStringLiteral("show-shortcuts"));
    keysAction->setText(QStringLiteral("Show the keyboard shortcuts"));
    keysAction->setProperty("componentName", QStringLiteral("arka-shortcuts"));
    keysAction->setProperty("componentDisplayName", QStringLiteral("ArkaOS Shortcuts"));
    const QList<QKeySequence> keysKeys{QKeySequence(Qt::META | Qt::Key_Slash)};
    KGlobalAccel::self()->setDefaultShortcut(keysAction, keysKeys);
    KGlobalAccel::self()->setShortcut(keysAction, keysKeys);
    QObject::connect(keysAction, &QAction::triggered, &app, [] {
        QProcess::startDetached(QStringLiteral("arka-hotkeys"), {});
    });

    if (clickMode)
        wheel.showPaths(files, editMode);
    return app.exec();
}

#include "main.moc"
