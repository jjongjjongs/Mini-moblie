package com.jjongjjongs.minimobile;

import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.content.pm.PackageInstaller;
import android.widget.Toast;

/**
 * Hears back from the {@link PackageInstaller} session {@link Updater} commits.
 *
 * <p>The system does not install straight away: it asks the person to confirm,
 * and tells the app so by sending this broadcast with a confirm {@link Intent}
 * to launch. Once they answer, the same broadcast carries whether it went
 * through. The receiver only forwards the prompt and reports the result; the
 * updated app relaunches on a successful install.
 */
public final class InstallReceiver extends BroadcastReceiver {
    @Override
    public void onReceive(Context context, Intent intent) {
        if (!Updater.INSTALL_ACTION.equals(intent.getAction())) {
            return;
        }
        int status = intent.getIntExtra(PackageInstaller.EXTRA_STATUS, PackageInstaller.STATUS_FAILURE);
        switch (status) {
            case PackageInstaller.STATUS_PENDING_USER_ACTION:
                // The system's own confirm screen - launch it as a new task,
                // since a broadcast has no window of its own.
                Intent confirm = intent.getParcelableExtra(Intent.EXTRA_INTENT);
                if (confirm != null) {
                    confirm.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
                    try {
                        context.startActivity(confirm);
                    } catch (Exception e) {
                        toast(context, "설치 화면을 열 수 없어요.");
                    }
                }
                break;
            case PackageInstaller.STATUS_SUCCESS:
                toast(context, "업데이트를 설치했어요.");
                break;
            case PackageInstaller.STATUS_FAILURE_ABORTED:
                // The person backed out of the confirm screen: nothing to say.
                break;
            default:
                String message = intent.getStringExtra(PackageInstaller.EXTRA_STATUS_MESSAGE);
                toast(context, "설치에 실패했어요" + (message != null ? " (" + message + ")" : "") + ".");
                break;
        }
    }

    private static void toast(Context context, String text) {
        Toast.makeText(context.getApplicationContext(), text, Toast.LENGTH_LONG).show();
    }
}
