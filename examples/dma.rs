// TIM2 更新リクエストが入るたびに PWM の Duty を DMA で書き換える。

#![no_std]
#![no_main]

// pick a panicking behavior
use panic_halt as _; // you can put a breakpoint on `rust_begin_unwind` to catch panics
                     // use panic_abort as _; // requires nightly
                     // use panic_itm as _; // logs messages over ITM; requires ITM support
                     // use panic_semihosting as _; // logs messages to the host stderr; requires a debugger

// cortex-m コア共通の機能を提供
use cortex_m;

// cortex-m コア向けのスタートアップ処理を提供
// メモリの初期化から例外テーブルのシンボル登録（リセット以外はダミーの定義）まで実施してくれる。
use cortex_m_rt::entry;

// このデバイスクレートをuseすることで、割り込みベクタテーブルのシンボル定義が自動登録される。
// 依存にデバイスクレート追加後、これ無しでビルドすると、cortex-m-rtの "device" features
// 　がONになり、テーブル定義が空になるので怒られる。（OFFの時はダミーの定義入れてくれる）
// デバイスクレートの "rt" features を外せば "device" がONにされないので回避できるが、
// 　自前でテーブル定義する必要が出てくる。
//　（デバイス固有のペリフェラルアクセスだけ利用したい場合用だが、あまり使う機会は無さそう）
use stm32f4::stm32f446;

// interrupt マクロ が使えるようになる
// 割り込み関数の定義に必要
// （デフォルトは何もしないことが定義されていて、そこに上書きする感じ）
use stm32f4::stm32f446::interrupt;

// const と static の違いはメモリ上に固定の位置を持つかどうか。
// const変数への参照は常に同じアドレスを指すとは限らない。
// 2%(1 - 1 = 0) -> 100%(50 - 1 = 49) -> 2%(1 - 1 = 0)
static DUTY_TABLE: [u32; 100] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25,
    26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49,
    49, 48, 47, 46, 45, 44, 43, 42, 41, 40, 39, 38, 37, 36, 35, 34, 33, 32, 31, 30, 29, 28, 27, 26,
    25, 24, 23, 22, 21, 20, 19, 18, 17, 16, 15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0,
];

// クロックの初期設定を実施
// SYSCLK: HSE(ST-Link 8MHz) -> PLL -> 180MHz
// APB1: 45MHz
fn config_clock(peripheral: &stm32f4::stm32f446::Peripherals) {
    // HSEはBypassモード(ST-Linkからの 8 MHz を使える)
    peripheral.RCC.cr.modify(|_, w| w.hsebyp().bypassed());
    // HSE ON
    peripheral.RCC.cr.modify(|_, w| w.hseon().on());
    // HSE の準備完了待ち
    while peripheral.RCC.cr.read().hserdy().is_not_ready() {}

    // クロック設定
    // PLL の ソースクロックをHSE(後述の通り8MHz)とする
    peripheral.RCC.pllcfgr.modify(|_, w| w.pllsrc().hse());
    // PLL へ入るクロックを分周（1 ~ 2MHzなので、2MHzとする）
    peripheral
        .RCC
        .pllcfgr
        .modify(|_, w| unsafe { w.pllm().bits(4) });
    // 上の続きで逓倍できる(100 ~ 432MHzなので、360MHzとする)
    peripheral
        .RCC
        .pllcfgr
        .modify(|_, w| unsafe { w.plln().bits(180) });
    // PLL クロック確定(最大180MHzなので、180MHzとする)
    // ちなみにPはシステムクロック、Rは、QはUSBなど...別々に運用できる
    peripheral.RCC.pllcfgr.modify(|_, w| w.pllp().div2());

    // PLL ON
    peripheral.RCC.cr.modify(|_, w| w.pllon().on());
    // PLL の準備完了待ち
    while peripheral.RCC.cr.read().pllrdy().is_not_ready() {}

    // フラッシュの読み出し遅延設定（180MHzだと5WS）
    peripheral.FLASH.acr.modify(|_, w| w.latency().ws5());

    // PLLPをシステムクロックとして使う設定
    peripheral.RCC.cfgr.modify(|_, w| w.sw().pll());
    while peripheral.RCC.cfgr.read().sws().is_pll() == false {}

    // APB1を分周（最大45MHz）
    peripheral.RCC.cfgr.modify(|_, w| w.ppre1().div4());

    // APB2を分周（最大90MHz）
    peripheral.RCC.cfgr.modify(|_, w| w.ppre2().div2());
}

fn config_dma(peripheral: &stm32f4::stm32f446::Peripherals) {
    // DMA1 設定 stream7-channel3
    peripheral.DMA1.st[7].cr.modify(|_, w| w.chsel().bits(3)); // channel3

    // 初期値のダイレクトモードではMSIZEは無視されPSIZEとなるが一応セットする。
    peripheral.DMA1.st[7].cr.modify(|_, w| {
        w.msize()
            .bits32() // メモリデータサイズ4byte
            .psize()
            .bits32() // ペリフェラルデータサイズ4byte
            .minc()
            .incremented() // メモリアドレスインクリメント
            .circ()
            .enabled() // サーキュラモード有効
            .dir()
            .memory_to_peripheral() // メモリからペリフェラルへ転送
    });

    peripheral.DMA1.st[7]
        .ndtr
        .modify(|_, w| w.ndt().bits(DUTY_TABLE.len() as u16)); // 転送データ数
    peripheral.DMA1.st[7]
        .par
        .write(|w| unsafe { w.pa().bits(0x4000_0000 + 0x34) }); // 転送先ペリフェラルアドレス指定（TIM2_CCR1）
    peripheral.DMA1.st[7]
        .m0ar
        .write(|w| unsafe { w.m0a().bits(DUTY_TABLE.as_ptr() as u32) }); // 転送元メモリアドレス指定

    peripheral.DMA1.st[7].cr.modify(|_, w| w.en().enabled()); // DMA1 ストリーム7有効化
}

fn config_tim(peripheral: &stm32f4::stm32f446::Peripherals) {
    // TIM2 設定（クロックはAPB1 * 2 = 90MHz）
    peripheral
        .TIM2
        .ccmr1_output()
        .modify(|_, w| w.oc1pe().enabled()); // CCR1 プリロード有効化
    peripheral.TIM2.cr1.modify(|_, w| w.arpe().enabled()); // ARR 自動プリロード有効化（これがないと再ロードできないので1パルスで止まる）
    peripheral.TIM2.psc.write(unsafe { |w| w.bits(18000 - 1) }); // プリスケーラ（何クロックで1カウントか設定）
    peripheral.TIM2.arr.write(unsafe { |w| w.bits(50 - 1) }); // オートリロードレジスタ（カウント値設定）, 50Hz
    peripheral
        .TIM2
        .ccmr1_output()
        .modify(|_, w| w.oc1m().pwm_mode1()); // PWM mode 1
    peripheral.TIM2.ccr1.write(unsafe { |w| w.bits(0) }); // Duty 2%
    peripheral.TIM2.egr.write(|w| w.ug().update()); // 更新生成（プリロード値を初期化）
    peripheral.TIM2.ccer.modify(|_, w| w.cc1e().set_bit()); // OC出力有効化
    peripheral.TIM2.dier.modify(|_, w| w.ude().enabled()); // 更新DMAリクエスト有効化
}

#[entry]
fn main() -> ! {
    let peripheral = stm32f446::Peripherals::take().unwrap();

    config_clock(&peripheral);

    // 各機能へのクロック入力設定
    peripheral.RCC.ahb1enr.modify(|_, w| w.gpioaen().enabled());
    peripheral.RCC.apb1enr.modify(|_, w| w.tim2en().enabled());
    peripheral.RCC.ahb1enr.modify(|_, w| w.dma1en().enabled());

    // setting LD2(GPIOA-5)
    peripheral.GPIOA.moder.modify(|_, w| w.moder5().alternate());
    peripheral.GPIOA.afrl.modify(|_, w| w.afrl5().af1()); // TIM2-ch1 を選択

    config_dma(&peripheral);

    config_tim(&peripheral);

    peripheral.TIM2.cr1.modify(|_, w| w.cen().enabled()); // カウント開始

    loop {}
}
