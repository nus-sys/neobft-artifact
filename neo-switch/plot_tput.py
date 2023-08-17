#!/usr/bin/python3
'''
Usage: python3 plot_tput.py FILENAME
'''

import sys
import os
import matplotlib
import matplotlib.pyplot as plt
import numpy as np


hmac_number = float(sys.argv[1])
fpga_number = float(sys.argv[2])

matplotlib.rcParams['pdf.fonttype'] = 42
matplotlib.rcParams['font.family'] = 'sans-serif'
matplotlib.rcParams['font.sans-serif'] = ['Nimbus Roman']

num = 64
num_replica = [ i for i in range(4, num+4, 4)]
hmac_tput = []
peak_tput = hmac_number
for i, replica in enumerate(num_replica):
    hmac_tput.append(peak_tput/(i+1))
fpga_tput = [fpga_number for i in num_replica]

print(num_replica)
print(hmac_tput)
print(fpga_tput)

plt.plot(num_replica, fpga_tput, label='AOM-PK', marker="^")
plt.plot(num_replica, hmac_tput, label='AOM-HM', marker=".")
# plt.plot(num_replica, hmac_tput, label='Neo-HM')
# plt.plot(num_replica, fpga_tput, label='Neo-PK')

plt.tick_params(direction='in')
# plt.yscale('log')
# plt.yticks([100, 1000, 10000])
plt.annotate(str(fpga_number) + "Mpps", (2,3),fontsize=14)
plt.annotate(str(hmac_number) + "Mpps", (5,75),fontsize=14)

plt.xticks(fontsize=14)
plt.yticks(fontsize=14)

plt.xlabel('Number of Receivers',fontsize=14)
plt.ylabel('Throughput (Mpps)', fontsize=14)

plt.legend(fontsize=14)

plt.tight_layout()
# plt.savefig('results/tput.png'.format(sys.argv[1]))
plt.savefig('results/tput.pdf'.format(sys.argv[1]))
print(">> Done! See: results/tput.pdf")

exit()

# print("25.0th", int(np.percentile(data, 25)), "us")
# print("50.0th", int(np.percentile(data, 50)), "us")
# print("99.0th ", int(np.percentile(data, 99)), "us")
# print("99.9th", int(np.percentile(data, 99.9)), "us")
# print("99.99th", int(np.percentile(data, 99.99)), "us")
# print("99.999th", int(np.percentile(data, 99.999)), "us")

# y = 1. * np.arange(len(data)) / (len(data) - 1)
# plt.plot(data, y)

# plt.savefig('results/{}.png'.format(os.path.basename(sys.argv[1])))

# # plt.show()
# # sns.kdeplot(data = data, cumulative = True, label = "fpga")
# # plt.legend()