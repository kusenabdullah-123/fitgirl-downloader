# FitDownloader

FitDownloader adalah aplikasi desktop berbasis **Tauri v2** + **React/Preact** untuk scraping dan mengunduh file secara otomatis (terutama untuk format link FitGirl repack via Datanodes). Aplikasi ini dilengkapi dengan integrasi Selenium/Headless Chromium untuk mem-bypass perlindungan Cloudflare dan tombol unduhan yang dilindungi timer JavaScript.

## Fitur Utama

- **Direct Link Extraction:** Membaca link-link unduhan langsung dari halaman repack.
- **Auto Download Queue:** Mengunduh file secara antrian (*queue*) otomatis.
- **Headless Bypass:** Menjalankan instance Chrome tersembunyi untuk melompati timer (contoh: Datanodes countdown).
- **Progress Tracking:** Memantau kecepatan unduhan secara real-time, sisa indikator, dan nama file yang sedang diunduh.
- **Cross-Platform Storage:** Mendukung pemilihan lokasi unduhan dan binari *browser engine* melalui Dialog File *native* di Linux, Windows, dan macOS.

---

## Prasyarat (*Prerequisites*)

Pastikan sistem operasi Anda telah memasang lingkungan pengembangan Rust dan Node.js:
- [Node.js](https://nodejs.org/en/) (v18+)
- [Rust](https://www.rust-lang.org/tools/install)
- [Tauri v2 CLI](https://v2.tauri.app/start/prerequisites/)

Bila Anda menjalankan aplikasi ini di OS Linux (seperti Ubuntu/Debian), pastikan dependensi webkit terpasang:
```bash
sudo apt update
sudo apt install libwebkit2gtk-4.1-dev \
    build-essential \
    curl \
    wget \
    file \
    libssl-dev \
    libgtk-3-dev \
    libayatana-appindicator3-dev \
    librsvg2-dev
```

---

## Cara Menjalankan untuk Pengembangan (Development)

Untuk masuk ke mode *development* dengan *hot-reload*:
```bash
# Install dependencies frontend JS/TS
npm install

# Jalankan server Tauri Dev
npm run tauri dev
```

---

## Cara Melakukan Build (Rilis ke Eksekutabel)

Tauri bergantung pada lingkungan OS secara langsung (Native Windowing System). Anda tidak mem-build antarmuka satu paket, tetapi memanfaatkan `WebView2` di Windows dan `WebKit` di Linux. 

Oleh karena itu, ada beberapa target langkah bergantung OS (*Operating System*) Anda:

### 1. Build untuk Linux (`.AppImage` atau `.deb`)
Dari terminal Linux, sangat direkomendasikan untuk langsung menjalankan perintah:
```bash
npm run tauri build
```
File rilis *binary* murni (`fitgirl-downloader`) tanpa sistem installer akan tersedia di folder:
```text
src-tauri/target/release/fitgirl-downloader
```
*(Anda dapat mem-bypass file instalasi installer dengan argument: `npm run tauri build -- --no-bundle`)*

### 2. Build untuk Windows (`.exe` / `.msi`)
Pilihan terbaik dan paling stabil untuk mengeluarkan file `.exe` Windows adalah: **Menjalankan kompilasi (*build*) ini langsung di lingkungan OS Windows**.  

Bila Anda me-run project ini di *Windows (PC asli / Virtual Machine)*, pastikan fitur C++ (Build Tools for Visual Studio 2022) diaktifkan, lalu jalankan perintah percis sepert linux di *CMD/PowerShell*:
```powershell
npm run tauri build
```

Pilihan rilis `.exe` dan paket program installer (.msi) akan ter-generate di:
```text
src-tauri/target/release/bundle/
```

### 3. Build Windows dari Komputer Linux (Cross-Compile) ⚠️ *Warning!* ⚠️
Jika Anda berniat melakukan konversi rilis Windows dari Linux Anda, ini disebut **Cross-Compiling**. Cara ini cukup *advanced* karena dependensi tertentu (seperti OpenSSL dari crate reqwest) terkadang bentrok:

1. **Install MinGW Toolchain:**
   ```bash
   sudo apt install mingw-w64 nsis
   ```
2. **Tambahkan Target ke Rustup:**
   ```bash
   rustup target add x86_64-pc-windows-gnu
   ```
3. **Setelan Cargo Configuration:**
   Buat folder `.cargo/config.toml` di dalam proyek, isikan dengan Linker GNU:
   ```toml
   [target.x86_64-pc-windows-gnu]
   linker = "x86_64-w64-mingw32-gcc"
   ```
4. **Mulai Lakukan Build:**
   ```bash
   npm run tauri build -- --target x86_64-pc-windows-gnu
   ```

*(Catatan: Langkah 3 sering kali terkendala isu compability pustaka C++. Sangat kami rokemendasikan Anda menggunakan CI/CD seperti **GitHub Actions** jika ingin target ke beraneka jenis OS secara gratis!).*

---

## Cara Penggunaan FitDownloader

1. Buka aplikasi, masukkan Link utama (*parent URL* dari repack web) pada kolom masukan paling atas.
2. (Opsional) Klik **Browse** di kolom bawah URL untuk mengarahkan path khusus untuk letak executable Chrome/Edge milik Anda bila sistem tak bisa mendeteksi `headless_chrome` / Tauri *webview* standar.
3. (Opsional) Ubah **Custom Download Directory** dengan mengklik `Browse` juga.
4. Klik **Fetch Links**.
5. Pilih link mana yang ingin diunduh pada kolom *Fetched*, tekan **Save**.
6. Terakhir, pada kolom *Queue* klik tombol Play (ikon panah / DL) di sebelah link atau tombol cerdas **Auto DL All** untuk mengunduhnya sekaligus.
7. Biarkan aplikasi memantau file tersebut secara terus-menerus! Semua intervensi timer & blokir web akan ditangani di belakang layar.
