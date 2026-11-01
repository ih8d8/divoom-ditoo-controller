use std::collections::VecDeque;

#[test]
fn test_checksum_algorithm() {
    // Test framing and checksum as verified with hardware
    // GetVolume packet: opcode 0x09, length 3, checksum = 3 + 0 + 9 = 12 (0x0c)
    let payload: &[u8] = &[];
    let length = (payload.len() + 3) as u16;
    let cmd: u8 = 0x09;
    let mut body = Vec::new();
    body.extend_from_slice(&length.to_le_bytes());
    body.push(cmd);
    body.extend_from_slice(payload);

    let checksum: u16 = body.iter().map(|&b| b as u16).sum();
    assert_eq!(checksum, 0x0c);

    let mut packet = vec![0x01];
    packet.extend_from_slice(&body);
    packet.extend_from_slice(&checksum.to_le_bytes());
    packet.push(0x02);

    assert_eq!(packet, vec![0x01, 0x03, 0x00, 0x09, 0x0c, 0x00, 0x02]);
}

#[test]
fn test_set_datetime_packet() {
    // Command 0x18, payload has 7 bytes: yy, century, mm, dd, hh, min, sec
    let payload = vec![26, 20, 10, 30, 21, 0, 0];
    let length = (payload.len() + 3) as u16; // 10
    let cmd: u8 = 0x18;

    let mut body = Vec::new();
    body.extend_from_slice(&length.to_le_bytes());
    body.push(cmd);
    body.extend_from_slice(&payload);

    let checksum: u16 = body.iter().map(|&b| b as u16).sum();
    let expected_sum = 10 + 24 + 26 + 20 + 10 + 30 + 21;
    assert_eq!(checksum, expected_sum);
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
enum Priority {
    Low = 0,
    Normal = 1,
    High = 2,
    Urgent = 3,
}

struct TestItem {
    name: &'static str,
    priority: Priority,
}

#[test]
fn test_priority_queue_ordering() {
    let mut queue = VecDeque::new();

    let items = vec![
        TestItem {
            name: "normal1",
            priority: Priority::Normal,
        },
        TestItem {
            name: "low1",
            priority: Priority::Low,
        },
        TestItem {
            name: "urgent1",
            priority: Priority::Urgent,
        },
        TestItem {
            name: "high1",
            priority: Priority::High,
        },
        TestItem {
            name: "normal2",
            priority: Priority::Normal,
        },
    ];

    for item in items {
        let mut insert_idx = queue.len();
        for (idx, q_item) in queue.iter().enumerate() {
            let q: &TestItem = q_item;
            if item.priority > q.priority {
                insert_idx = idx;
                break;
            }
        }
        queue.insert(insert_idx, item);
    }

    assert_eq!(queue[0].name, "urgent1");
    assert_eq!(queue[1].name, "high1");
    assert_eq!(queue[2].name, "normal1");
    assert_eq!(queue[3].name, "normal2");
    assert_eq!(queue[4].name, "low1");
}

#[test]
fn test_daemon_state_enqueue_capacity_and_eviction() {
    use divoom_ditoo_controller::config::Config;
    use divoom_ditoo_controller::daemon::{
        DaemonState, MAX_QUEUE_SIZE, NotificationItem, Priority,
    };
    use std::time::Instant;
    let mut state = DaemonState {
        config: Config::default(),
        device: None,
        device_addr: None,
        queue: VecDeque::new(),
        last_notification: None,
        is_displaying: false,
        laptop_battery: None,
        is_battery_override: false,
    };

    // Fill queue to MAX_QUEUE_SIZE with Normal priority items
    for i in 0..MAX_QUEUE_SIZE {
        state.enqueue(NotificationItem {
            message: format!("msg_{i}"),
            title: None,
            icon: None,
            canvas: None,
            duration: 5.0,
            priority: Priority::Normal,
            sound: false,
            created_at: Instant::now(),
        });
    }
    assert_eq!(state.queue.len(), MAX_QUEUE_SIZE);

    // Enqueue an Urgent item when queue is full
    state.enqueue(NotificationItem {
        message: "urgent_override".into(),
        title: None,
        icon: None,
        canvas: None,
        duration: 5.0,
        priority: Priority::Urgent,
        sound: false,
        created_at: Instant::now(),
    });

    // Queue must stay capped at MAX_QUEUE_SIZE
    assert_eq!(state.queue.len(), MAX_QUEUE_SIZE);
    // Highest priority item must be at front
    assert_eq!(state.queue[0].priority, Priority::Urgent);
    assert_eq!(state.queue[0].message, "urgent_override");
}
