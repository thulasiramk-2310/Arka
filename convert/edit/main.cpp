// arka-edit: the small window behind the "Edit" entries: crop, black out parts
// of a picture, put a picture on a background, trim sound or video.
//   arka-edit crop|redact|background|trim FILE
// It only collects what you chose. The work is done by arka-convert, which
// runs it in the sandbox and never changes the original.

#include <QDir>
#include <QMouseEvent>
#include <QFile>
#include <QFileInfo>
#include <QGuiApplication>
#include <QImageReader>
#include <QProcess>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickStyle>
#include <QQuickWindow>
#include <QTimer>
#include <QUrl>

class Edit : public QObject
{
    Q_OBJECT
    Q_PROPERTY(QString tool MEMBER m_tool CONSTANT)
    Q_PROPERTY(QString fileName READ fileName CONSTANT)
    Q_PROPERTY(QUrl fileUrl READ fileUrl CONSTANT)
    Q_PROPERTY(qreal duration READ duration CONSTANT)

public:
    QString m_tool;
    QString m_file;

    QString fileName() const { return QFileInfo(m_file).fileName(); }
    QUrl fileUrl() const { return QUrl::fromLocalFile(m_file); }

    // Length of a sound or video in seconds, asked of arka-convert (sandboxed). 0 if unknown.
    qreal duration() const
    {
        if (m_tool != QLatin1String("trim"))
            return 0;
        QProcess p;
        p.start(QStringLiteral("arka-convert"), {QStringLiteral("--duration"), m_file});
        if (!p.waitForFinished(15000))
            return 0;
        return QString::fromUtf8(p.readAllStandardOutput()).trimmed().toDouble();
    }

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
        QImageReader r(m_file);
        r.setAutoTransform(true);
        QSize size = r.size();   // the size stored in the file, before its rotation tag
        if (r.transformation() & QImageIOHandler::TransformationRotate90)
            size.transpose();    // a sideways photo is shown, and edited, upright
        return size;
    }

    // Hand the choice to arka-convert and close. args are plain strings, never a shell line.
    Q_INVOKABLE void apply(const QStringList &args)
    {
        QProcess::startDetached(QStringLiteral("arka-convert"), QStringList{m_tool} + QStringList{m_file} + args);
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

    const QStringList a = app.arguments();
    static const QStringList tools{"crop", "redact", "background", "trim"};
    if (a.size() != 3 || !tools.contains(a[1]) || !QFileInfo(a[2]).isFile()) {
        qCritical("usage: arka-edit crop|redact|background|trim FILE");
        return 2;
    }

    Edit edit;
    edit.m_tool = a[1];
    edit.m_file = QFileInfo(a[2]).absoluteFilePath();

    QQmlApplicationEngine engine;
    engine.rootContext()->setContextProperty(QStringLiteral("edit"), &edit);
    engine.load(QUrl(QStringLiteral("qrc:/Edit.qml")));
    auto *window = engine.rootObjects().isEmpty() ? nullptr
                                                  : qobject_cast<QQuickWindow *>(engine.rootObjects().first());
    if (!window)
        return 1;

    // For testing without a screen. ARKA_EDIT_EVENTS replays mouse events as
    // "p X Y" (press), "m X Y" (move), "r X Y" (release), ";"-separated, in window
    // coordinates; ARKA_EDIT_SCREENSHOT then saves the window and quits.
    const QStringList events = qEnvironmentVariable("ARKA_EDIT_EVENTS").split(';', Qt::SkipEmptyParts);
    int delay = 500;
    for (const QString &e : events) {
        const QStringList f = e.trimmed().split(' ', Qt::SkipEmptyParts);
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
