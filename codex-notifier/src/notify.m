#import <Cocoa/Cocoa.h>

@interface NotifierDelegate : NSObject <NSUserNotificationCenterDelegate>
@end

@implementation NotifierDelegate
- (BOOL)userNotificationCenter:(NSUserNotificationCenter *)center shouldPresentNotification:(NSUserNotification *)notification {
    return YES;
}
@end

int main(int argc, const char * argv[]) {
    @autoreleasepool {
        if (argc < 2) {
            fprintf(stderr, "Usage: notify <title> [message] [subtitle]\n");
            return 1;
        }
        NSString *title = [NSString stringWithUTF8String:argv[1]];
        NSString *message = argc > 2 ? [NSString stringWithUTF8String:argv[2]] : @"";
        NSString *subtitle = argc > 3 ? [NSString stringWithUTF8String:argv[3]] : nil;

        NotifierDelegate *delegate = [[NotifierDelegate alloc] init];
        NSUserNotificationCenter *center = [NSUserNotificationCenter defaultUserNotificationCenter];
        center.delegate = delegate;

        NSUserNotification *notification = [[NSUserNotification alloc] init];
        notification.title = title;
        notification.informativeText = message;
        if (subtitle && [subtitle length] > 0) {
            notification.subtitle = subtitle;
        }
        notification.soundName = NSUserNotificationDefaultSoundName;

        [center deliverNotification:notification];

        // Ticking the runloop allows XPC delivery to macOS usernoted
        [[NSRunLoop currentRunLoop] runUntilDate:[NSDate dateWithTimeIntervalSinceNow:0.15]];
    }
    return 0;
}
