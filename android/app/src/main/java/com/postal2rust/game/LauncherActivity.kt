package com.postal2rust.game

import android.app.AlertDialog
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.os.StatFs
import android.view.Gravity
import android.view.ViewGroup
import android.widget.AdapterView
import android.widget.ArrayAdapter
import android.widget.Button
import android.widget.LinearLayout
import android.widget.ProgressBar
import android.widget.ScrollView
import android.widget.Spinner
import android.widget.TextView
import android.widget.Toast
import androidx.activity.ComponentActivity
import androidx.activity.result.contract.ActivityResultContracts
import androidx.core.content.FileProvider
import androidx.documentfile.provider.DocumentFile
import java.io.File
import java.io.FileOutputStream

/**
 * Launcher of the app: the first screen. It
 *  * copies the player's own POSTAL 2 install (picked with the system folder picker) into the app's
 *    external files folder, which survives app updates (only uninstalling removes it);
 *  * sets the frame-rate cap and starts the game (NativeActivity -> libp2android.so);
 *  * opens the touch-controls editor in the game.
 *
 * No game files are shipped in the APK. Everything the game reads comes from the copy.
 */
class LauncherActivity : ComponentActivity() {

    private val ui = Handler(Looper.getMainLooper())
    private lateinit var status: TextView
    private lateinit var info: TextView
    private lateinit var progress: ProgressBar
    private lateinit var progressText: TextView
    private lateinit var playButton: Button
    private lateinit var copyButton: Button
    private lateinit var deleteButton: Button
    private lateinit var editorButton: Button
    private lateinit var fpsSpinner: Spinner
    private lateinit var soundButton: Button

    @Volatile private var copying = false
    @Volatile private var cancelCopy = false

    private val pickFolder = registerForActivityResult(ActivityResultContracts.OpenDocumentTree()) { uri ->
        if (uri != null) confirmCopy(uri)
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        buildUi()
        refresh()
        recordExits()
    }

    /** Reads how the earlier runs of the game ended (native crashes included) into the logs. */
    private fun recordExits() {
        Thread {
            val added = try {
                ExitReasons.record(this, logsDir())
            } catch (e: Exception) {
                0
            }
            if (added > 0) ui.post { toast("Найдено записей о завершении игры: $added. Они попадут в лог.") }
        }.start()
    }

    override fun onResume() {
        super.onResume()
        if (!copying) refresh()
    }

    // ---- paths ------------------------------------------------------------------------------

    /** `.../Android/data/com.postal2rust.game/files`: kept on updates, removed on uninstall. */
    private fun dataDir(): File = getExternalFilesDir(null) ?: filesDir

    private fun gameDir(): File = File(dataDir(), "POSTAL2")

    /** Written by the native side (`android_log.rs`): game.txt, logcat.txt, crash.txt and *-previous. */
    private fun logsDir(): File = File(dataDir(), "logs")

    private fun hasLogs(): Boolean = logsDir().isDirectory && (logsDir().listFiles()?.isNotEmpty() == true)

    // ---- logs -------------------------------------------------------------------------------

    private fun shareLog() {
        if (!hasLogs()) {
            toast("Логов пока нет: сначала запустите игру")
            return
        }
        Thread {
            try {
                val file = GameLogs.bundle(this, logsDir())
                val uri = FileProvider.getUriForFile(this, "$packageName.logs", file)
                ui.post {
                    val send = Intent(Intent.ACTION_SEND).apply {
                        type = "text/plain"
                        putExtra(Intent.EXTRA_STREAM, uri)
                        putExtra(Intent.EXTRA_SUBJECT, "POSTAL 2 Rust: лог")
                        clipData = ClipData.newRawUri("log", uri)
                        addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
                    }
                    startActivity(Intent.createChooser(send, "Отправить лог"))
                }
            } catch (e: Exception) {
                ui.post { toast("Не удалось подготовить лог: ${e.message}") }
            }
        }.start()
    }

    private fun copyLog() {
        if (!hasLogs()) {
            toast("Логов пока нет: сначала запустите игру")
            return
        }
        Thread {
            try {
                val text = GameLogs.tailText(this, logsDir(), COPY_CHARS)
                ui.post {
                    val clipboard = getSystemService(CLIPBOARD_SERVICE) as ClipboardManager
                    clipboard.setPrimaryClip(ClipData.newPlainText("postal2rust-log", text))
                    toast("Скопировано ${text.length} символов: конец лога")
                }
            } catch (e: Exception) {
                ui.post { toast("Не удалось скопировать лог: ${e.message}") }
            }
        }.start()
    }

    private fun isInstalled(dir: File): Boolean =
        File(dir, "System/Postal2.ini").isFile || File(dir, "System/Default.ini").isFile

    // ---- UI ---------------------------------------------------------------------------------

    private fun buildUi() {
        val pad = dp(20)
        val column = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(pad, pad, pad, pad)
        }
        column.addView(text("POSTAL 2 Rust/Bevy", 26f, bold = true))
        column.addView(text("Порт движка на Android. Игровые файлы берутся из вашей копии POSTAL 2: в APK их нет.", 14f))

        status = text("", 16f, bold = true).also { column.addView(it, spaced(top = 16)) }
        info = text("", 13f).also { column.addView(it, spaced(top = 4)) }

        progress = ProgressBar(this, null, android.R.attr.progressBarStyleHorizontal).apply {
            max = 1000
            visibility = android.view.View.GONE
        }
        column.addView(progress, spaced(top = 12))
        progressText = text("", 13f).also {
            it.visibility = android.view.View.GONE
            column.addView(it, spaced(top = 4))
        }

        copyButton = button("1. Выбрать папку POSTAL 2 и скопировать") { pickFolder.launch(null) }
        column.addView(copyButton, spaced(top = 18))

        playButton = button("2. Играть") { play(editor = false) }
        column.addView(playButton, spaced(top = 8))

        editorButton = button("Настроить сенсорное управление") { play(editor = true) }
        column.addView(editorButton, spaced(top = 8))

        column.addView(text("Частота кадров", 15f, bold = true), spaced(top = 22))
        fpsSpinner = Spinner(this)
        val fpsValues = FPS_VALUES
        fpsSpinner.adapter = ArrayAdapter(this, android.R.layout.simple_spinner_dropdown_item, fpsValues.map { "$it FPS" })
        fpsSpinner.setSelection(fpsValues.indexOf(prefs().getInt(KEY_FPS, 60)).coerceAtLeast(0))
        fpsSpinner.onItemSelectedListener = object : AdapterView.OnItemSelectedListener {
            override fun onItemSelected(parent: AdapterView<*>?, view: android.view.View?, position: Int, id: Long) {
                prefs().edit().putInt(KEY_FPS, fpsValues[position]).apply()
                writeConfig()
            }
            override fun onNothingSelected(parent: AdapterView<*>?) {}
        }
        column.addView(fpsSpinner, spaced(top = 4))
        column.addView(
            text("60 FPS — цель по умолчанию: одинаково ровно на всех телефонах. 90 и 120 — для экранов с высокой частотой, но тяжелее для батареи и температуры.", 12f),
            spaced(top = 4),
        )

        column.addView(text("Звук и диагностика", 15f, bold = true), spaced(top = 22))
        soundButton = button(soundLabel()) {
            val next = !prefs().getBoolean(KEY_SOUND, true)
            prefs().edit().putBoolean(KEY_SOUND, next).apply()
            soundButton.text = soundLabel()
            writeConfig()
        }
        column.addView(soundButton, spaced(top = 6))
        column.addView(button("Диагностика: пустая сцена Bevy (без игры)") { playEmptyScene() }, spaced(top = 6))
        column.addView(
            text("Если пустая сцена тоже падает — проблема в устройстве или драйвере. Если работает — в игре.", 12f),
            spaced(top = 4),
        )

        column.addView(text("Лог для отчёта об ошибке", 15f, bold = true), spaced(top = 22))
        column.addView(
            text("Если игра вылетела, отправьте лог: в нём запуск, сообщения игры и текст ошибки. Папка логов: ${logsDir().absolutePath}", 12f),
            spaced(top = 4),
        )
        column.addView(button("Отправить лог") { shareLog() }, spaced(top = 6))
        column.addView(button("Скопировать конец лога в буфер") { copyLog() }, spaced(top = 6))

        deleteButton = button("Удалить копию игры из приложения") { confirmDelete() }
        column.addView(deleteButton, spaced(top = 22))

        val version = try {
            packageManager.getPackageInfo(packageName, 0).versionName
        } catch (e: Exception) {
            "?"
        }
        column.addView(
            text(
                "Версия $version. Обновления ставятся поверх установленной версии — переустанавливать не нужно, копия игры и сохранения сохраняются.\n\nКопия игры: ${gameDir().absolutePath}",
                12f,
            ),
            spaced(top = 22),
        )

        setContentView(ScrollView(this).apply { addView(column, ViewGroup.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT)) })
    }

    private fun refresh() {
        val installed = isInstalled(gameDir())
        status.text = if (installed) "Игра готова к запуску" else "Игра не найдена: выберите папку POSTAL 2"
        info.text = if (installed) "Установлено в приложение: ${gameDir().absolutePath}" else "Папка должна содержать System, Maps, Textures (например, Steam: steamapps/common/POSTAL2Complete)."
        playButton.isEnabled = installed && !copying
        editorButton.isEnabled = installed && !copying
        copyButton.isEnabled = !copying
        deleteButton.isEnabled = !copying && gameDir().exists()
        fpsSpinner.isEnabled = !copying
    }

    private fun setBusy(busy: Boolean) {
        copying = busy
        progress.visibility = if (busy) android.view.View.VISIBLE else android.view.View.GONE
        progressText.visibility = if (busy) android.view.View.VISIBLE else android.view.View.GONE
        refresh()
    }

    // ---- copying ----------------------------------------------------------------------------

    /** The picked folder may be the install itself or a folder that contains it. */
    private fun findInstall(root: DocumentFile): DocumentFile? {
        if (root.findFile("System")?.let { it.findFile("Postal2.ini") ?: it.findFile("Default.ini") } != null) return root
        return root.listFiles().firstOrNull { child ->
            child.isDirectory && child.findFile("System")?.let { it.findFile("Postal2.ini") ?: it.findFile("Default.ini") } != null
        }
    }

    private fun confirmCopy(treeUri: Uri) {
        val root = DocumentFile.fromTreeUri(this, treeUri)
        val install = root?.let { findInstall(it) }
        if (install == null) {
            AlertDialog.Builder(this)
                .setMessage("В этой папке нет POSTAL 2 (нужна папка с System/Postal2.ini).")
                .setPositiveButton("OK", null)
                .show()
            return
        }
        AlertDialog.Builder(this)
            .setTitle("Копирование")
            .setMessage("Скопировать игру в приложение? Это займёт несколько гигабайт свободного места. Уже скопированные файлы будут пропущены.")
            .setPositiveButton("Копировать") { _, _ -> copyInstall(install) }
            .setNegativeButton("Отмена", null)
            .show()
    }

    private fun copyInstall(install: DocumentFile) {
        cancelCopy = false
        setBusy(true)
        progress.progress = 0
        progressText.text = "Поиск файлов…"
        Thread {
            try {
                val files = ArrayList<Pair<String, DocumentFile>>()
                collect(install, "", files)
                val total = files.sumOf { it.second.length() }.coerceAtLeast(1L)
                val free = StatFs(dataDir().path).availableBytes
                if (free < total + total / 20) {
                    throw IllegalStateException("Нужно около ${total shr 20} МБ, свободно ${free shr 20} МБ")
                }
                val dst = gameDir()
                dst.mkdirs()
                var done = 0L
                var lastUi = 0L
                for ((rel, doc) in files) {
                    if (cancelCopy) break
                    val target = File(dst, rel)
                    target.parentFile?.mkdirs()
                    val size = doc.length()
                    if (target.isFile && target.length() == size) {
                        // Copied by an earlier run (or an interrupted one): count it and go on.
                        done += size
                    } else {
                        contentResolver.openInputStream(doc.uri)?.use { input ->
                            FileOutputStream(target).use { out ->
                                val buf = ByteArray(1 shl 20)
                                while (true) {
                                    val n = input.read(buf)
                                    if (n < 0 || cancelCopy) break
                                    out.write(buf, 0, n)
                                    done += n
                                    val now = System.currentTimeMillis()
                                    if (now - lastUi > 150) {
                                        lastUi = now
                                        val frac = done.toDouble() / total
                                        ui.post { showProgress(frac, rel) }
                                    }
                                }
                            }
                        }
                    }
                }
                if (cancelCopy) {
                    ui.post { toast("Копирование остановлено. Продолжить можно в любой момент.") }
                } else {
                    ui.post { toast("Игра скопирована") }
                }
            } catch (e: Exception) {
                ui.post { toast("Ошибка копирования: ${e.message}") }
            } finally {
                ui.post { setBusy(false) }
            }
        }.start()
    }

    private fun collect(dir: DocumentFile, prefix: String, out: MutableList<Pair<String, DocumentFile>>) {
        for (child in dir.listFiles()) {
            val name = child.name ?: continue
            val rel = if (prefix.isEmpty()) name else "$prefix/$name"
            if (child.isDirectory) collect(child, rel, out) else if (child.isFile) out += rel to child
        }
    }

    private fun showProgress(frac: Double, current: String) {
        progress.progress = (frac * 1000).toInt().coerceIn(0, 1000)
        progressText.text = "Копирование: ${(frac * 100).toInt()}%  $current"
    }

    private fun confirmDelete() {
        AlertDialog.Builder(this)
            .setMessage("Удалить скопированную игру из приложения? Сохранения тоже будут удалены.")
            .setPositiveButton("Удалить") { _, _ ->
                Thread {
                    gameDir().deleteRecursively()
                    ui.post { refresh() }
                }.start()
            }
            .setNegativeButton("Отмена", null)
            .show()
    }

    // ---- starting the game ------------------------------------------------------------------

    /** Starts Bevy with an empty scene (no game code), for diagnosis. See `empty_scene` in p2android. */
    private fun playEmptyScene() {
        writeConfig()
        File(dataDir(), "test_scene").writeText("1")
        startActivity(Intent(this, android.app.NativeActivity::class.java))
    }

    private fun play(editor: Boolean) {
        if (!isInstalled(gameDir())) return
        File(dataDir(), "test_scene").delete()
        writeConfig()
        val flag = File(dataDir(), "start_touch_editor")
        if (editor) flag.writeText("1") else flag.delete()
        startActivity(Intent(this, android.app.NativeActivity::class.java))
    }

    private fun soundLabel(): String =
        if (prefs().getBoolean(KEY_SOUND, true)) "Звук: включён (нажмите, чтобы выключить для проверки)"
        else "Звук: выключен (для проверки падения)"

    /** Settings the native side reads at start: `fps=N` and `audio=on|off`. Rendering is always Vulkan. */
    private fun writeConfig() {
        val fps = prefs().getInt(KEY_FPS, 60)
        val audio = if (prefs().getBoolean(KEY_SOUND, true)) "on" else "off"
        File(dataDir(), "android.cfg").writeText("# written by the launcher\nfps=$fps\naudio=$audio\n")
    }

    // ---- helpers ----------------------------------------------------------------------------

    private fun prefs() = getSharedPreferences("launcher", MODE_PRIVATE)

    private fun toast(msg: String) = Toast.makeText(this, msg, Toast.LENGTH_LONG).show()

    private fun dp(v: Int): Int = (v * resources.displayMetrics.density).toInt()

    private fun text(s: String, sp: Float, bold: Boolean = false) = TextView(this).apply {
        text = s
        textSize = sp
        if (bold) setTypeface(typeface, android.graphics.Typeface.BOLD)
    }

    private fun button(label: String, onClick: () -> Unit) = Button(this).apply {
        text = label
        isAllCaps = false
        setOnClickListener { onClick() }
    }

    private fun spaced(top: Int) = LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT).apply {
        topMargin = dp(top)
        gravity = Gravity.CENTER_HORIZONTAL
    }

    companion object {
        private const val KEY_FPS = "fps"
        private const val KEY_SOUND = "sound"
        private val FPS_VALUES = listOf(30, 60, 90, 120)
        /** How much of the log goes to the clipboard (the full log is in the shared file). */
        private const val COPY_CHARS = 100_000
    }
}
