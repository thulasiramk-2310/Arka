// arka-edit: the small window behind the "Edit" entries: crop or black out parts
// of a picture or a video, draw on a picture, put a picture on a background, and
// trim, split or beep over parts of a sound or video.
//   arka-edit crop|redact|annotate|background|trim|split|bleep FILE
// It only collects what you chose. The work is done by arka-convert, which
// runs it in the sandbox and never changes the original.

#include <QDateTime>
#include <QDesktopServices>
#include <QDir>
#include <QLocale>
#include <QSettings>
#include <QMouseEvent>
#include <QFile>
#include <QFileInfo>
#include <QFont>
#include <QFontDatabase>
#include <QJsonDocument>
#include <QJsonArray>
#include <QJsonObject>
#include <QKeyEvent>
#include <QGuiApplication>
#include <QImageReader>
#include <QProcess>
#include <QIcon>
#include <QQmlApplicationEngine>
#include <QQuickImageProvider>
#include <QQmlContext>
#include <QQuickStyle>
#include <QQuickWindow>
#include <QStandardPaths>
#include <QTemporaryDir>
#include <QTemporaryFile>
#include <QTimer>
#include <QUrl>

// Theme icons for the tool buttons, by name ("image://icon/draw-freehand"), as the wheel does.
class IconProvider : public QQuickImageProvider
{
public:
    IconProvider() : QQuickImageProvider(QQuickImageProvider::Pixmap) {}
    QPixmap requestPixmap(const QString &id, QSize *size, const QSize &requested) override
    {
        const int side = requested.width() > 0 ? requested.width() : 32;
        QPixmap pm = QIcon::fromTheme(id).pixmap(side, side);
        if (size)
            *size = pm.size();
        return pm;
    }
};

class Edit : public QObject
{
    Q_OBJECT
    Q_PROPERTY(QString tool MEMBER m_tool CONSTANT)
    Q_PROPERTY(QString fileName READ fileName CONSTANT)
    Q_PROPERTY(QUrl fileUrl READ fileUrl CONSTANT)
    Q_PROPERTY(qreal duration READ duration CONSTANT)
    Q_PROPERTY(bool isVideo READ isVideo CONSTANT)
    Q_PROPERTY(QUrl pictureUrl READ pictureUrl CONSTANT)
    Q_PROPERTY(QUrl waveUrl READ waveUrl CONSTANT)
    Q_PROPERTY(bool hasFrame READ hasFrame CONSTANT)
    Q_PROPERTY(QVariantMap hidden READ hidden NOTIFY hiddenChanged)
    Q_PROPERTY(bool autoFind MEMBER m_autoFind CONSTANT)
    Q_PROPERTY(bool inspectorOpen READ inspectorOpen WRITE setInspectorOpen NOTIFY inspectorOpenChanged)

public:
    QString m_tool;
    QString m_file;
    bool m_autoFind = false;    // "Hide private text": Black out that starts by finding private text
    QString m_frame;            // for a video: one frame, saved where m_tmp is
    QString m_wave;             // for sound and video: the waveform, saved where m_tmp is
    QTemporaryDir m_tmp;
    mutable qreal m_duration = -1;
    QVariantMap m_hidden;       // the hidden details mat2 finds, read once before the window shows

    QString fileName() const { return QFileInfo(m_file).fileName(); }
    QUrl fileUrl() const { return QUrl::fromLocalFile(m_file); }

    bool isVideo() const
    {
        static const QStringList video{"mp4", "mkv", "webm", "mov", "avi", "m4v", "ogv"};
        return video.contains(QFileInfo(m_file).suffix().toLower());
    }

    // What the picture tools draw on: the picture itself, or a frame of the video.
    QUrl pictureUrl() const { return m_frame.isEmpty() ? fileUrl() : QUrl::fromLocalFile(m_frame); }
    QUrl waveUrl() const { return m_wave.isEmpty() ? QUrl() : QUrl::fromLocalFile(m_wave); }
    bool hasFrame() const { return !m_frame.isEmpty(); }

    // The sound drawn as a waveform, for the timeline. A file with no sound simply has none.
    void grabWave()
    {
        if (!m_tmp.isValid())
            return;
        const QString out = m_tmp.filePath(QStringLiteral("wave.png"));
        QProcess p;
        p.start(QStringLiteral("arka-convert"), {QStringLiteral("--waveform"), m_file, out,
                                                 dark() ? QStringLiteral("1") : QStringLiteral("0")});
        if (p.waitForFinished(60000) && p.exitCode() == 0 && QFileInfo(out).isFile())
            m_wave = out;
    }
    QString picturePath() const { return m_frame.isEmpty() ? m_file : m_frame; }

    // Length of a sound or video in seconds, asked of arka-convert (sandboxed). 0 if unknown.
    qreal duration() const
    {
        if (m_duration >= 0)
            return m_duration;
        m_duration = 0;
        if (m_tool != QLatin1String("trim") && m_tool != QLatin1String("split") && m_tool != QLatin1String("bleep")
            && !isVideo())
            return 0;
        QProcess p;
        p.start(QStringLiteral("arka-convert"), {QStringLiteral("--duration"), m_file});
        if (p.waitForFinished(15000))
            m_duration = QString::fromUtf8(p.readAllStandardOutput()).trimmed().toDouble();
        return m_duration;
    }

    // A frame a second in (or from the middle of a short clip), for cropping or covering a video.
    bool grabFrame()
    {
        if (!m_tmp.isValid())
            return false;
        const qreal len = duration();
        const qreal at = len > 0 ? qMin<qreal>(1.0, len / 2) : 0;
        const QString out = m_tmp.filePath(QStringLiteral("frame.png"));
        QProcess p;
        p.start(QStringLiteral("arka-convert"), {QStringLiteral("--frame"), m_file, QString::number(at, 'f', 3), out});
        if (!p.waitForFinished(60000) || p.exitCode() != 0 || !QFileInfo(out).isFile())
            return false;
        m_frame = out;
        return true;
    }

    // The inspector panel on the right: open or closed, remembered for next time.
    bool inspectorOpen() const { return QSettings(QStringLiteral("arkaos"), QStringLiteral("arka-edit")).value("inspector", true).toBool(); }
    void setInspectorOpen(bool open)
    {
        if (open == inspectorOpen())
            return;
        QSettings(QStringLiteral("arkaos"), QStringLiteral("arka-edit")).setValue("inspector", open);
        Q_EMIT inspectorOpenChanged();
    }

    // What the inspector lists: plain facts read from the file itself, nothing guessed.
    Q_INVOKABLE QVariantMap details() const
    {
        const QFileInfo fi(m_file);
        const QLocale loc;
        static const QStringList audio{"mp3", "wav", "flac", "ogg", "oga", "opus", "m4a", "aac"};
        const QString ext = fi.suffix().toLower();
        QString kind = isVideo() ? QStringLiteral("Video") : audio.contains(ext) ? QStringLiteral("Sound") : QStringLiteral("Picture");
        QVariantMap d;
        d["kind"] = kind;
        d["format"] = ext.toUpper();
        d["size"] = loc.formattedDataSize(fi.size(), 1, QLocale::DataSizeIecFormat);
        // A date that reads as one everywhere: "5 Nov 2026, 13:35", month in the user's language.
        d["changed"] = loc.toString(fi.lastModified(), QStringLiteral("d MMM yyyy, HH:mm"));
        d["folder"] = shortFolder(fi);
        return d;
    }

    // The folder with the home part written as "~". Compared by real path: on ArkaOS /home is a
    // link to /var/home, so the same folder can be spelled both ways.
    static QString shortFolder(const QFileInfo &fi)
    {
        const QString folder = fi.dir().canonicalPath().isEmpty() ? fi.absolutePath() : fi.dir().canonicalPath();
        const QString home = QDir(QDir::homePath()).canonicalPath();
        if (!home.isEmpty() && (folder == home || folder.startsWith(home + QLatin1Char('/'))))
            return QStringLiteral("~") + folder.mid(home.size());
        return QDir::toNativeSeparators(folder);
    }

    QVariantMap hidden() const { return m_hidden; }

    // Asks arka-convert (which reads them in the sandbox with mat2's library) what the file carries.
    // In the background: the window opens at once and the panel fills in when the answer comes.
    void readHidden()
    {
        m_hidden = {{QStringLiteral("checking"), true}};
        auto *p = new QProcess(this);
        connect(p, &QProcess::finished, this, [this, p](int code) {
            m_hidden = code == 0 ? QJsonDocument::fromJson(p->readAllStandardOutput()).object().toVariantMap()
                                 : QVariantMap{};
            p->deleteLater();
            Q_EMIT hiddenChanged();
        });
        connect(p, &QProcess::errorOccurred, this, [this, p](QProcess::ProcessError e) {
            if (e != QProcess::FailedToStart)
                return;
            m_hidden = {};
            p->deleteLater();
            Q_EMIT hiddenChanged();
        });
        p->start(QStringLiteral("arka-convert"), {QStringLiteral("--details"), m_file});
        // A reading that hangs is stopped after a minute; "finished" then clears "Checking…".
        QTimer::singleShot(60000, p, [p] {
            if (p->state() != QProcess::NotRunning)
                p->kill();
        });
    }

    // Saves a copy without them, next to the file (arka-convert clean, the same as the menu entry).
    Q_INVOKABLE bool removeHidden() const
    {
        return QProcess::startDetached(QStringLiteral("arka-convert"), {QStringLiteral("clean"), m_file});
    }

    Q_INVOKABLE void openFile() const { QDesktopServices::openUrl(fileUrl()); }

    // Opens the folder with this file selected, as the file manager's own "Show in folder" does.
    Q_INVOKABLE void showInFolder() const
    {
        QProcess::startDetached(QStringLiteral("dbus-send"),
                                {"--session", "--dest=org.freedesktop.FileManager1", "--type=method_call",
                                 "/org/freedesktop/FileManager1", "org.freedesktop.FileManager1.ShowItems",
                                 "array:string:" + fileUrl().toString(QUrl::FullyEncoded), "string:"});
    }

    // Black out: ask arka-convert to read the picture (OCR, in the sandbox) and suggest boxes over
    // email addresses, card and phone numbers. Runs in the background; privateFound() brings the boxes.
    Q_INVOKABLE void findPrivate()
    {
        auto *p = new QProcess(this);
        // If arka-convert can't even start, say so instead of leaving the button busy for ever.
        connect(p, &QProcess::errorOccurred, this, [this, p](QProcess::ProcessError e) {
            if (e != QProcess::FailedToStart)
                return;
            p->deleteLater();
            Q_EMIT privateFound({}, QStringLiteral("the text reader could not be started"));
        });
        connect(p, &QProcess::finished, this, [this, p](int code) {
            const QByteArray out = p->readAllStandardOutput();
            const QString err = QString::fromUtf8(p->readAllStandardError()).trimmed().section(QLatin1String(": "), 1);
            p->deleteLater();
            if (code != 0) {
                // Plain words, not the tool's own message (e.g. "Could not initialize tesseract").
                const QString why = err.contains(QLatin1String("tesseract"), Qt::CaseInsensitive)
                                        ? QStringLiteral("text recognition is not available on this computer")
                                        : QStringLiteral("the picture could not be read");
                Q_EMIT privateFound({}, why);
                return;
            }
            Q_EMIT privateFound(QJsonDocument::fromJson(out).array().toVariantList(), QString());
        });
        p->start(QStringLiteral("arka-convert"), {QStringLiteral("--find-private"), picturePath()});
        QTimer::singleShot(120000, p, [p] {   // reading a picture never takes this long; stop it
            if (p->state() != QProcess::NotRunning)
                p->kill();
        });
    }

    // Work still running when the window closes (OCR, details) is stopped, not left behind.
    ~Edit() override
    {
        for (QProcess *p : findChildren<QProcess *>()) {
            p->disconnect();
            p->kill();
            p->waitForFinished(2000);
        }
    }

Q_SIGNALS:
    void inspectorOpenChanged();
    void hiddenChanged();
    void privateFound(const QVariantList &found, const QString &error);

public:
    Q_INVOKABLE bool dark() const
    {
        QFile f(QDir::homePath() + QStringLiteral("/.config/arka-look"));
        if (f.open(QIODevice::ReadOnly))
            return f.readAll().trimmed() != "light";
        return true;
    }

    // The picture's size after its rotation tag is applied: the size the tools work in.
    Q_INVOKABLE QSize imageSize() const
    {
        QImageReader r(picturePath());
        r.setAutoTransform(true);
        QSize size = r.size();   // the size stored in the file, before its rotation tag
        if (r.transformation() & QImageIOHandler::TransformationRotate90)
            size.transpose();    // a sideways photo is shown, and edited, upright
        return size;
    }

    // Hand the choice to arka-convert and close. args are plain strings, never a shell line.
    Q_INVOKABLE void apply(const QStringList &args)
    {
        const QString command = m_tool == QLatin1String("split") ? QStringLiteral("split-media") : m_tool;
        QStringList pass = args;
        if (m_tool == QLatin1String("annotate") && !args.isEmpty() && args.first().size() > 64 * 1024) {
            // A long drawing does not fit in one command-line argument: hand it over as a file in the
            // private runtime folder (only this user can read it); arka-convert removes it once read.
            QTemporaryFile f(QStandardPaths::writableLocation(QStandardPaths::RuntimeLocation)
                             + QStringLiteral("/arka-edit-XXXXXX.json"));
            f.setAutoRemove(false);
            if (!f.open() || f.write(args.first().toUtf8()) < 0) {
                qCritical("arka-edit: could not save the drawing for arka-convert");
                return;
            }
            pass = {QLatin1Char('@') + f.fileName()};
        }
        if (!QProcess::startDetached(QStringLiteral("arka-convert"), QStringList{command} + QStringList{m_file} + pass)) {
            qCritical("arka-edit: could not start arka-convert");
            return;   // keep the window, so the work is not lost
        }
        QGuiApplication::quit();
    }
};

int main(int argc, char **argv)
{
    // Flat drawing needs no GPU; software keeps the window light and works in a VM.
    if (qEnvironmentVariableIsEmpty("QT_QUICK_BACKEND"))
        qputenv("QT_QUICK_BACKEND", "software");
    QGuiApplication app(argc, argv);
    app.setApplicationName(QStringLiteral("arka-edit"));
    app.setDesktopFileName(QStringLiteral("org.arkaos.edit"));
    QQuickStyle::setStyle(QStringLiteral("Basic"));
    // The type ArkaOS's own apps use (Inter for words, Noto Serif for titles), when it is installed.
    for (const QString &family : {QStringLiteral("Inter"), QStringLiteral("Inter Variable")}) {
        if (QFontDatabase::hasFamily(family)) {   // packaged under either name
            QFont f(family);
            f.setPointSizeF(QGuiApplication::font().pointSizeF());
            QGuiApplication::setFont(f);
            break;
        }
    }

    const QStringList a = app.arguments();
    static const QStringList tools{"crop", "redact", "annotate", "background", "trim", "split", "bleep"};
    const bool find = a.size() == 4 && a[1] == QLatin1String("redact") && a[3] == QLatin1String("--find-private");
    if ((a.size() != 3 && !find) || !tools.contains(a[1]) || !QFileInfo(a[2]).isFile()) {
        qCritical("usage: arka-edit crop|redact|annotate|background|trim|split|bleep FILE\n"
                  "       arka-edit redact FILE --find-private");
        return 2;
    }

    Edit edit;
    edit.m_tool = a[1];
    edit.m_file = QFileInfo(a[2]).absoluteFilePath();
    edit.m_autoFind = find;
    edit.duration();   // asked once, before the window is drawn, so no property read waits on it later
    edit.readHidden();
    const bool timeTool = edit.m_tool == QLatin1String("trim") || edit.m_tool == QLatin1String("split")
                          || edit.m_tool == QLatin1String("bleep");
    if ((edit.m_tool == QLatin1String("crop") || edit.m_tool == QLatin1String("redact")) && edit.isVideo()
        && !edit.grabFrame()) {
        qCritical("arka-edit: could not read a frame of %s", qPrintable(edit.fileName()));
        return 1;
    }
    if (timeTool) {
        edit.grabWave();
        if (edit.isVideo())
            edit.grabFrame();   // a poster for the details panel; the tool works without it
    }

    // Icons come from the desktop's theme; without one (a bare session), Breeze in the matching shade.
    QStringList iconDirs = QIcon::themeSearchPaths();
    for (const QString &dir : QStandardPaths::locateAll(QStandardPaths::GenericDataLocation, QStringLiteral("icons"),
                                                         QStandardPaths::LocateDirectory))
        if (!iconDirs.contains(dir))
            iconDirs << dir;
    QIcon::setThemeSearchPaths(iconDirs);
    if (QIcon::themeName().isEmpty() || QIcon::themeName() == QLatin1String("hicolor"))
        QIcon::setThemeName(edit.dark() ? QStringLiteral("breeze-dark") : QStringLiteral("breeze"));

    QQmlApplicationEngine engine;
    engine.addImageProvider(QStringLiteral("icon"), new IconProvider);
    engine.rootContext()->setContextProperty(QStringLiteral("edit"), &edit);
    engine.load(QUrl(QStringLiteral("qrc:/Edit.qml")));
    auto *window = engine.rootObjects().isEmpty() ? nullptr
                                                  : qobject_cast<QQuickWindow *>(engine.rootObjects().first());
    if (!window) {
        qCritical("arka-edit: the window could not be loaded (see the messages above)");
        return 1;
    }

    // For testing without a screen. ARKA_EDIT_EVENTS replays mouse events as
    // "p X Y" (press), "m X Y" (move), "r X Y" (release), ";"-separated, in window
    // coordinates, and keys as "t TEXT" (types it) and "k enter". ARKA_EDIT_SCREENSHOT
    // then saves the window and quits.
    const QStringList events = qEnvironmentVariable("ARKA_EDIT_EVENTS").split(';', Qt::SkipEmptyParts);
    if (!events.isEmpty())
        window->requestActivate();   // keyboard shortcuts only reach the active window, also without a screen
    int delay = 500;
    for (const QString &e : events) {
        const QString line = e.trimmed();
        if (line == QLatin1String("k tab")) {
            QTimer::singleShot(delay += 40, [window] {
                QKeyEvent down(QEvent::KeyPress, Qt::Key_Tab, Qt::NoModifier);
                QKeyEvent up(QEvent::KeyRelease, Qt::Key_Tab, Qt::NoModifier);
                QCoreApplication::sendEvent(window, &down);
                QCoreApplication::sendEvent(window, &up);
            });
            continue;
        }
        if (line.startsWith(QLatin1String("t ")) || line == QLatin1String("k enter")) {
            const bool enter = line == QLatin1String("k enter");
            const QString text = enter ? QString() : line.mid(2);
            QTimer::singleShot(delay += 40, [window, text, enter] {
                const auto send = [window](int key, const QString &t) {
                    QKeyEvent down(QEvent::KeyPress, key, Qt::NoModifier, t);
                    QKeyEvent up(QEvent::KeyRelease, key, Qt::NoModifier, t);
                    QCoreApplication::sendEvent(window, &down);
                    QCoreApplication::sendEvent(window, &up);
                };
                if (enter)
                    send(Qt::Key_Return, QStringLiteral("\r"));
                for (const QChar c : text)
                    send(c.isDigit() ? Qt::Key_0 + c.digitValue() : c == ':' ? Qt::Key_Colon : c == '.' ? Qt::Key_Period
                                                                                                       : Qt::Key_A + (c.toLower().unicode() - 'a'),
                         QString(c));
            });
            continue;
        }
        const QStringList f = line.split(' ', Qt::SkipEmptyParts);
        if (f.size() != 3)
            continue;
        QTimer::singleShot(delay += 40, [window, f] {
            const QPointF p(f[1].toDouble(), f[2].toDouble());
            const auto type = f[0] == "p" ? QEvent::MouseButtonPress
                              : f[0] == "r" ? QEvent::MouseButtonRelease : QEvent::MouseMove;
            const auto buttons = f[0] == "r" ? Qt::NoButton : Qt::LeftButton;
            QMouseEvent ev(type, p, p, window->mapToGlobal(p), Qt::LeftButton, buttons, Qt::NoModifier);
            QCoreApplication::sendEvent(window, &ev);
        });
    }
    const QString shot = qEnvironmentVariable("ARKA_EDIT_SCREENSHOT");
    if (!shot.isEmpty())
        QTimer::singleShot(delay + 700, [window, shot] {
            window->grabWindow().save(shot);
            QGuiApplication::quit();
        });
    return app.exec();
}

#include "main.moc"
