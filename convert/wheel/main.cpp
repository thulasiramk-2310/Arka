// arka-wheel: the ArkaOS format wheel. Runs hidden for the whole session; the
// "Show the format wheel" shortcut (Meta+Alt+C by default, changeable in System
// Settings > Shortcuts) shows a ring of formats at the cursor while you drag a
// file. Drop the file on a format and arka-convert makes the copy next to the
// original. `arka-wheel --files FILE...` (the Dolphin right-click entry) shows
// the same ring for clicking. All conversion work is done by arka-convert.

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
#include <QRegion>
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
        out = {"png"};
    out.removeAll(ext == "jpeg" ? QStringLiteral("jpg") : ext); // not the format it already is
    return out;
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
    Q_SCRIPTABLE void ShowFiles(const QStringList &paths)
    {
        QList<QUrl> urls;
        for (const QString &p : paths)
            urls << QUrl::fromLocalFile(QFileInfo(p).absoluteFilePath());
        Q_EMIT opened(urls);
    }
    void showForDrag() { Q_EMIT opened({}); }

Q_SIGNALS:
    // Empty urls: wait for a dragged file. Otherwise: click mode for these files.
    void opened(const QList<QUrl> &urls);
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
    const bool clickMode = !files.isEmpty() && files.first() == QLatin1String("--files");
    if (clickMode)
        files.removeFirst();

    auto bus = QDBusConnection::sessionBus();
    if (bus.interface()->isServiceRegistered(Service)) {
        // Already running for this session: hand over the files, or do nothing.
        if (clickMode) {
            auto msg = QDBusMessage::createMethodCall(Service, Path, Service, QStringLiteral("ShowFiles"));
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

    auto *layer = LayerShellQt::Window::get(window);
    layer->setLayer(LayerShellQt::Window::LayerOverlay);
    layer->setAnchors(LayerShellQt::Window::Anchors(LayerShellQt::Window::AnchorTop | LayerShellQt::Window::AnchorBottom
                                                    | LayerShellQt::Window::AnchorLeft | LayerShellQt::Window::AnchorRight));
    layer->setExclusiveZone(-1);
    layer->setKeyboardInteractivity(LayerShellQt::Window::KeyboardInteractivityOnDemand);
    layer->setScope(QStringLiteral("arka-wheel"));

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

    if (clickMode)
        wheel.ShowFiles(files);
    return app.exec();
}

#include "main.moc"
